use crate::ecs::{Entity, World};
use crate::input::event::PointerCursor;
use crate::input::event::hit_test::hit_test;
use crate::input::feedback::{
    CursorFeedbackMode, CursorVisual, InputFeedback, OverlayCursor, write_overlay_layout,
};
use crate::render::command::DrawCommand;
use crate::types::{Color, Fixed, Rect};
use crate::ui::dirty::Dirty;
use crate::ui::render_system::LastDirtyRegions;
use crate::ui::view::PaintCtx;
use crate::ui::{Children, ComputedRect, IgnoreHitTest, Parent, Style, Widget, WidgetRoot};

const PRIMARY: Color = Color::rgb(88, 166, 255);

fn cursor_dot_rect(visual: &CursorVisual) -> Rect {
    // Larger radius while pressed gives a tactile "swell" without changing colour.
    let r = Fixed::from_int(if visual.down { 5 } else { 4 });
    Rect {
        x: visual.x - r,
        y: visual.y - r,
        w: r * Fixed::from_int(2),
        h: r * Fixed::from_int(2),
    }
}

fn current_visual(world: &World, cursor: PointerCursor) -> CursorVisual {
    let target = world
        .resource::<WidgetRoot>()
        .copied()
        .filter(|_| cursor.present)
        .and_then(|root| {
            world
                .resource::<crate::surface::DisplayInfo>()
                .and_then(|info| {
                    hit_test(world, root.0, cursor.x, cursor.y, info.width, info.height)
                })
        });
    let target_rect = target.and_then(|e| world.get::<ComputedRect>(e).map(|r| r.0));
    CursorVisual {
        present: cursor.present,
        x: cursor.x,
        y: cursor.y,
        down: cursor.down,
        target,
        target_rect,
    }
}

fn entity_target_rect(visual: &CursorVisual, mode: CursorFeedbackMode) -> Rect {
    match mode {
        CursorFeedbackMode::Dot => cursor_dot_rect(visual),
        CursorFeedbackMode::MagneticRect => visual
            .target_rect
            .map(|r| {
                let pad = Fixed::from_int(3);
                Rect {
                    x: r.x - pad,
                    y: r.y - pad,
                    w: r.w + pad * Fixed::from_int(2),
                    h: r.h + pad * Fixed::from_int(2),
                }
            })
            .unwrap_or_else(|| cursor_dot_rect(visual)),
    }
}

fn spawn_overlay_cursor(world: &mut World, root: Entity, initial_rect: Rect) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, Widget);
    world.insert(entity, OverlayCursor);
    world.insert(entity, IgnoreHitTest);
    world.insert(entity, Style::absolute_at(initial_rect));
    world.insert(entity, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(entity);
    } else {
        world.insert(root, Children(alloc::vec![entity]));
    }
    entity
}

fn world_has_layout_motion(world: &World) -> bool {
    let shifted = world
        .resource::<LastDirtyRegions>()
        .is_some_and(|r| !r.0.shifts.is_empty());
    let pending_scroll = crate::ui::render_system::has_pending_scroll(world);
    shifted || pending_scroll
}

pub(crate) fn cursor_feedback_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return Ok(());
    };
    if !feedback.cursor.enabled {
        return Ok(());
    }
    let Some(cursor) = world.resource::<PointerCursor>().copied() else {
        return Ok(());
    };
    if !cursor.present && feedback.cursor.entity.is_none() {
        return Ok(());
    }
    // event_seq doesn't tick on Move events, so it can't gate this
    // whole system; compare the raw pointer fields instead and skip
    // the ~3 ms hit_test in `current_visual` when nothing changed.
    //
    // The pointer-unchanged check alone misses one case: cursor stays
    // put while content under it scrolls or otherwise moves. The
    // already-cached `target_rect` then references entities whose
    // `ComputedRect` shifted, leaving the magnetic overlay frozen on
    // a position that no longer corresponds to anything. Force
    // recompute when the previous frame moved entities (any
    // `RegionShift` published) or when scroll containers carry
    // an offset different from their painted position.
    if feedback.cursor.entity.is_some()
        && feedback.cursor.current.present == cursor.present
        && feedback.cursor.current.x == cursor.x
        && feedback.cursor.current.y == cursor.y
        && feedback.cursor.current.down == cursor.down
        && !world_has_layout_motion(world)
    {
        return Ok(());
    }
    let visual = current_visual(world, cursor);
    let unchanged =
        feedback.cursor.last_event_seq == cursor.event_seq && feedback.cursor.current == visual;
    if unchanged {
        return Ok(());
    }

    feedback.cursor.current = visual;
    feedback.cursor.last_event_seq = cursor.event_seq;
    let rect = entity_target_rect(&visual, feedback.cursor.mode);

    let entity = if let Some(e) = feedback.cursor.entity {
        write_overlay_layout(world, e, rect);
        e
    } else {
        let Some(root) = world.resource::<WidgetRoot>().copied() else {
            // Visual change recorded but root not ready; commit and bail.
            world.insert_resource(feedback);
            return Ok(());
        };
        let e = spawn_overlay_cursor(world, root.0, rect);
        feedback.cursor.entity = Some(e);
        e
    };
    world.insert_resource(feedback);
    world.insert(entity, Dirty);
    Ok(())
}

crate::ecs::system::framework_system!(pub cursor_feedback_system, Normal);

#[crate::view(component = OverlayCursor, name = "input_feedback_cursor", priority = 90)]
fn cursor_render(feedback: res!(optional InputFeedback), paint: paint!()) {
    let Some(feedback) = feedback else {
        return;
    };
    if !feedback.cursor.enabled {
        return;
    }
    paint.set_background_handled(true);
    if !feedback.cursor.current.present {
        return;
    }
    let rect = paint.rect();
    let corner = rect.h / Fixed::from_int(2);
    match feedback.cursor.mode {
        CursorFeedbackMode::Dot => {
            emit_fill(paint, rect, corner, 220);
        }
        CursorFeedbackMode::MagneticRect => {
            if feedback.cursor.current.target_rect.is_some() {
                let rounded = Fixed::from_int(8);
                emit_fill(paint, rect, rounded, 48);
                emit_border(paint, rect, rounded, 160);
            } else {
                emit_fill(paint, rect, corner, 220);
            }
        }
    }
}

fn emit_fill(paint: &mut PaintCtx<'_>, rect: Rect, radius: Fixed, opa: u8) {
    paint.draw(&DrawCommand::Fill {
        area: rect,
        transform: paint.transform(),
        quad: paint.quad(),
        color: PRIMARY,
        radius,
        opa,
    });
}

fn emit_border(paint: &mut PaintCtx<'_>, rect: Rect, radius: Fixed, opa: u8) {
    paint.draw(&DrawCommand::Border {
        area: rect,
        transform: paint.transform(),
        quad: paint.quad(),
        color: PRIMARY,
        width: Fixed::ONE,
        radius,
        opa,
    });
}

pub use cursor_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::renderer::{RenderError, RenderFeature, Renderer};
    use crate::render::texture::ColorFormat;
    use crate::surface::DisplayInfo;
    use crate::types::{Dimension, Transform};
    use crate::ui::layout::{LayoutStyle, Position};
    use crate::ui::{HitTarget, Style};

    fn make_world() -> World {
        let mut app = crate::app::App::headless(128, 128);
        app.with_default_widgets();
        app.world
    }

    #[test]
    fn cursor_draw_failure_reaches_paint_context() {
        struct RejectRenderer;
        impl Renderer for RejectRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &crate::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, OverlayCursor);
        let mut feedback = InputFeedback::enabled();
        feedback.cursor.current.present = true;
        feedback.cursor.mode = CursorFeedbackMode::MagneticRect;
        feedback.cursor.current.target_rect = Some(Rect::new(0, 0, 16, 16));
        world.insert_resource(feedback);
        let style = Style::default();
        let rect = Rect::new(0, 0, 16, 16);
        let mut renderer = RejectRenderer;
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            crate::ui::theme::WidgetState::Enabled,
        );
        view().render().unwrap()(&world, entity, &mut paint);
        assert_eq!(
            paint.error(),
            Some(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
        world
            .resource_mut::<InputFeedback>()
            .unwrap()
            .cursor
            .current
            .present = false;
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            crate::ui::theme::WidgetState::Enabled,
        );
        view().render().unwrap()(&world, entity, &mut paint);
        assert_eq!(paint.error(), None);
        assert!(paint.background_handled());
    }

    fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(children) = world.get_mut::<Children>(p) {
                children.0.push(e);
            } else {
                world.insert(p, Children(alloc::vec![e]));
            }
        }
        e
    }

    fn root_with_target(world: &mut World) -> Entity {
        world.insert_resource(DisplayInfo {
            width: 128,
            height: 128,
            scale: Fixed::ONE,
            format: ColorFormat::RGBA8888,
        });
        world.insert_resource(InputFeedback::enabled());
        let root = spawn_widget(
            world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(128),
                    height: Dimension::px(128),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(target, HitTarget);
        world.insert_resource(WidgetRoot(root));
        crate::ui::render_system::try_update_layout(
            world,
            root,
            &crate::types::Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();
        root
    }

    #[test]
    fn cursor_system_lazy_spawns_overlay_on_first_pointer() {
        let mut world = make_world();
        root_with_target(&mut world);
        world.insert_resource(PointerCursor {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });

        assert!(
            world
                .resource::<InputFeedback>()
                .unwrap()
                .cursor
                .entity
                .is_none(),
            "no entity before pointer"
        );
        cursor_feedback_system(&mut world).unwrap();
        assert!(
            world
                .resource::<InputFeedback>()
                .unwrap()
                .cursor
                .entity
                .is_some(),
            "system should spawn overlay entity",
        );
    }

    #[test]
    fn cursor_system_marks_dirty_on_change() {
        let mut world = make_world();
        root_with_target(&mut world);
        world.insert_resource(PointerCursor {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });
        cursor_feedback_system(&mut world).unwrap();
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .entity
            .expect("spawned");
        // Walker normally consumes Dirty; assert it was set this frame.
        assert!(world.get::<Dirty>(entity).is_some());

        world.remove::<Dirty>(entity);
        world.insert_resource(PointerCursor {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::from_int(20),
            y: Fixed::from_int(20),
            down: false,
            event_seq: 2,
        });
        cursor_feedback_system(&mut world).unwrap();
        assert!(
            world.get::<Dirty>(entity).is_some(),
            "move should re-mark dirty"
        );
    }

    #[test]
    fn absent_cursor_does_not_spawn_and_leaving_invalidates_existing_overlay() {
        let mut world = make_world();
        root_with_target(&mut world);
        world.insert_resource(PointerCursor::default());
        cursor_feedback_system(&mut world).unwrap();
        assert!(
            world
                .resource::<InputFeedback>()
                .unwrap()
                .cursor
                .entity
                .is_none()
        );
        let cursor = world.resource_mut::<PointerCursor>().unwrap();
        cursor.present = true;
        cursor.x = 12.into();
        cursor.y = 14.into();
        cursor_feedback_system(&mut world).unwrap();
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .entity
            .unwrap();
        world.remove::<Dirty>(entity);
        world.resource_mut::<PointerCursor>().unwrap().present = false;
        cursor_feedback_system(&mut world).unwrap();
        let feedback = world.resource::<InputFeedback>().unwrap();
        assert!(!feedback.cursor.current.present);
        assert!(feedback.cursor.current.target.is_none());
        assert!(world.get::<Dirty>(entity).is_some());
        world.remove::<Dirty>(entity);
        cursor_feedback_system(&mut world).unwrap();
        assert!(world.get::<Dirty>(entity).is_none());
    }

    #[test]
    fn cursor_system_recomputes_when_layout_shifts_under_static_pointer() {
        let mut world = make_world();
        let root = root_with_target(&mut world);
        world.insert_resource(PointerCursor {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::from_int(40),
            y: Fixed::from_int(40),
            down: false,
            event_seq: 1,
        });
        // Configure MagneticRect mode so the overlay tracks target_rect.
        let mut fb = world.resource::<InputFeedback>().copied().unwrap();
        fb.cursor.mode = CursorFeedbackMode::MagneticRect;
        world.insert_resource(fb);
        cursor_feedback_system(&mut world).unwrap();
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .entity
            .expect("spawned");
        let cached_target = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .current
            .target_rect;
        assert!(
            cached_target.is_some(),
            "magnetic rect should latch onto target"
        );
        world.remove::<Dirty>(entity);

        // Move the target entity under a static pointer (simulates a
        // scroll-blit shifting children below). Without the layout-
        // motion gate, the short-circuit returns on stale (x, y, down)
        // and leaves the magnetic overlay frozen on the old position.
        let target = world
            .get::<Children>(root)
            .map(|c| c.0[0])
            .expect("child target");
        let prev_rect = world.get::<ComputedRect>(target).map(|r| r.0).unwrap();
        let style = world.get_mut::<Style>(target).expect("target style");
        style.layout.position = Position::Absolute;
        style.layout.left = Dimension::px(0);
        style.layout.top = Dimension::px(20);
        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &crate::types::Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();

        // Layout-motion signal that triggers the gate: a published
        // shift on the previous frame.
        use crate::ui::dirty::{DirtyRegions, RegionShift};
        let mut plan = DirtyRegions::new();
        plan.shifts.push(RegionShift {
            area: crate::types::Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(128),
                Fixed::from_int(128),
            ),
            dx: Fixed::ZERO,
            dy: Fixed::from_int(-4),
        });
        world.insert_resource(crate::ui::render_system::LastDirtyRegions(plan));
        cursor_feedback_system(&mut world).unwrap();
        let new_target = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .current
            .target_rect;
        assert_eq!(
            new_target.map(|r| r.y),
            Some(prev_rect.y + Fixed::from_int(20)),
            "magnetic rect must follow shifted target, not stay on cached y",
        );
        assert!(
            world.get::<Dirty>(entity).is_some(),
            "overlay must Dirty so its position updates this frame",
        );
    }

    #[test]
    fn cursor_system_skips_when_visual_unchanged() {
        let mut world = make_world();
        root_with_target(&mut world);
        world.insert_resource(PointerCursor {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });
        cursor_feedback_system(&mut world).unwrap();
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .entity
            .expect("spawned");
        world.remove::<Dirty>(entity);

        // Same event_seq + same visual → no work.
        cursor_feedback_system(&mut world).unwrap();
        assert!(world.get::<Dirty>(entity).is_none());
    }
}
