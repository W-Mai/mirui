use crate::ecs::{Entity, World};
use crate::event::PointerCursor;
use crate::event::hit_test::hit_test;
use crate::event::scroll::ScrollDelta;
use crate::feedback::{
    CursorFeedbackMode, CursorVisual, InputFeedback, OverlayCursor, write_overlay_layout,
};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Color, Fixed, Rect};
use crate::widget::dirty::Dirty;
use crate::widget::render_system::LastDirtyRegions;
use crate::widget::view::{View, ViewCtx};
use crate::widget::{Children, ComputedRect, IgnoreHitTest, Parent, Style, Widget, WidgetRoot};

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
    let target = world.resource::<WidgetRoot>().copied().and_then(|root| {
        world
            .resource::<crate::surface::DisplayInfo>()
            .and_then(|info| hit_test(world, root.0, cursor.x, cursor.y, info.width, info.height))
    });
    let target_rect = target.and_then(|e| world.get::<ComputedRect>(e).map(|r| r.0));
    CursorVisual {
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
    let pending_scroll = world.has_any_by_id(core::any::TypeId::of::<ScrollDelta>());
    shifted || pending_scroll
}

#[crate::system(order = NORMAL)]
pub fn cursor_feedback_system(world: &mut World) {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return;
    };
    if !feedback.cursor.enabled {
        return;
    }
    let Some(cursor) = world.resource::<PointerCursor>().copied() else {
        return;
    };
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
    // `RegionShift` published) or when scroll containers carry a
    // pending `ScrollDelta` this frame.
    if feedback.cursor.entity.is_some()
        && feedback.cursor.current.x == cursor.x
        && feedback.cursor.current.y == cursor.y
        && feedback.cursor.current.down == cursor.down
        && !world_has_layout_motion(world)
    {
        return;
    }
    let visual = current_visual(world, cursor);
    let unchanged =
        feedback.cursor.last_event_seq == cursor.event_seq && feedback.cursor.current == visual;
    if unchanged {
        return;
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
            return;
        };
        let e = spawn_overlay_cursor(world, root.0, rect);
        feedback.cursor.entity = Some(e);
        e
    };
    world.insert_resource(feedback);
    world.insert(entity, Dirty);
}

fn cursor_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if world.get::<OverlayCursor>(entity).is_none() {
        return;
    }
    let Some(feedback) = world.resource::<InputFeedback>() else {
        return;
    };
    if !feedback.cursor.enabled {
        return;
    }
    ctx.bg_handled = true;
    let corner = rect.h / Fixed::from_int(2);
    match feedback.cursor.mode {
        CursorFeedbackMode::Dot => {
            emit_fill(renderer, ctx, rect, corner, 220);
        }
        CursorFeedbackMode::MagneticRect => {
            if feedback.cursor.current.target_rect.is_some() {
                let rounded = Fixed::from_int(8);
                emit_fill(renderer, ctx, rect, rounded, 48);
                emit_border(renderer, ctx, rect, rounded, 160);
            } else {
                emit_fill(renderer, ctx, rect, corner, 220);
            }
        }
    }
}

fn emit_fill(renderer: &mut dyn Renderer, ctx: &ViewCtx, rect: &Rect, radius: Fixed, opa: u8) {
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: PRIMARY,
            radius,
            opa,
        },
        ctx.clip,
    );
}

fn emit_border(renderer: &mut dyn Renderer, ctx: &ViewCtx, rect: &Rect, radius: Fixed, opa: u8) {
    renderer.draw(
        &DrawCommand::Border {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: PRIMARY,
            width: Fixed::ONE,
            radius,
            opa,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("input_feedback_cursor", 90, cursor_render)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutStyle;
    use crate::render::texture::ColorFormat;
    use crate::surface::DisplayInfo;
    use crate::types::Dimension;
    use crate::widget::Style;

    fn make_world() -> World {
        let mut app = crate::app::App::headless(128, 128);
        app.with_default_widgets();
        app.world
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
        spawn_widget(
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
        world.insert_resource(WidgetRoot(root));
        crate::widget::render_system::update_layout(
            world,
            root,
            &crate::types::Viewport::new(128, 128, Fixed::ONE),
        );
        root
    }

    #[test]
    fn cursor_system_lazy_spawns_overlay_on_first_pointer() {
        let mut world = make_world();
        root_with_target(&mut world);
        world.insert_resource(PointerCursor {
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
        cursor_feedback_system(&mut world);
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
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });
        cursor_feedback_system(&mut world);
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
            x: Fixed::from_int(20),
            y: Fixed::from_int(20),
            down: false,
            event_seq: 2,
        });
        cursor_feedback_system(&mut world);
        assert!(
            world.get::<Dirty>(entity).is_some(),
            "move should re-mark dirty"
        );
    }

    #[test]
    fn cursor_system_recomputes_when_layout_shifts_under_static_pointer() {
        let mut world = make_world();
        let root = root_with_target(&mut world);
        world.insert_resource(PointerCursor {
            x: Fixed::from_int(40),
            y: Fixed::from_int(40),
            down: false,
            event_seq: 1,
        });
        // Configure MagneticRect mode so the overlay tracks target_rect.
        let mut fb = world.resource::<InputFeedback>().copied().unwrap();
        fb.cursor.mode = CursorFeedbackMode::MagneticRect;
        world.insert_resource(fb);
        cursor_feedback_system(&mut world);
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
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(
                prev_rect.x,
                prev_rect.y + Fixed::from_int(20),
                prev_rect.w,
                prev_rect.h,
            )),
        );

        // Layout-motion signal that triggers the gate: a published
        // shift on the previous frame.
        use crate::widget::dirty::{DirtyRegions, RegionShift};
        world.insert_resource(crate::widget::render_system::LastDirtyRegions(
            DirtyRegions {
                rects: alloc::vec![],
                shifts: alloc::vec![RegionShift {
                    area: crate::types::Rect::new(
                        Fixed::ZERO,
                        Fixed::ZERO,
                        Fixed::from_int(128),
                        Fixed::from_int(128),
                    ),
                    dx: Fixed::ZERO,
                    dy: Fixed::from_int(-4),
                }],
            },
        ));
        cursor_feedback_system(&mut world);
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
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });
        cursor_feedback_system(&mut world);
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .entity
            .expect("spawned");
        world.remove::<Dirty>(entity);

        // Same event_seq + same visual → no work.
        cursor_feedback_system(&mut world);
        assert!(world.get::<Dirty>(entity).is_none());
    }
}
