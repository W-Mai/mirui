extern crate std;
use super::*;
use crate::input::event::scroll::components::ScrollOffset;
use crate::types::Dimension;
use crate::ui::dirty::Dirty;
use crate::ui::layout::LayoutStyle;
use crate::ui::{Children, Parent, Style, Widget};

fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let e = world.spawn_empty();
    world.insert(e, Widget);
    world.insert(e, style);
    if let Some(p) = parent {
        world.insert(e, Parent(p));
        if let Some(children) = world.get_mut::<Children>(p) {
            children.0.push(e);
        } else {
            world.insert(p, Children(std::vec![e]));
        }
    }
    e
}

fn px_style(w: i32, h: i32) -> Style {
    Style {
        layout: LayoutStyle {
            width: Dimension::Px(Fixed::from_int(w)),
            height: Dimension::Px(Fixed::from_int(h)),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn painted(world: &mut World, root: Entity, viewport: &Viewport) {
    collect_dirty_regions(world, root, viewport).unwrap();
    seed_prev_rects(world, root, viewport);
    world.clear_subtree_dirty(root);
}

fn move_from_painted(
    world: &mut World,
    root: Entity,
    viewport: &Viewport,
    list: Entity,
    old_y: i32,
) {
    let current = *world.get::<ScrollOffset>(list).unwrap();
    world.get_mut::<ScrollOffset>(list).unwrap().y = Fixed::from_int(old_y);
    painted(world, root, viewport);
    world.insert(list, current);
    world.invalidate_visual(list);
}

#[test]
fn painted_offset_growth_emits_negative_scroll_op_and_bottom_strip() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(20),
        },
    );
    world.insert(list, Dirty);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 17);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

    assert_eq!(plan.shifts.len(), 1, "should emit one RegionShift");
    let sop = &plan.shifts[0];
    assert_eq!(sop.dx, Fixed::ZERO);
    assert_eq!(
        sop.dy,
        Fixed::from_int(-3),
        "framebuffer shifts opposite to ScrollOffset growth"
    );
    assert_eq!(sop.area.h, Fixed::from_int(100));

    // Bottom strip = (0, 100-3=97, 128, 3)
    let has_strip = plan.rects.iter().any(|r| {
        r.h == Fixed::from_int(3) && r.y == Fixed::from_int(97) && r.w == Fixed::from_int(128)
    });
    assert!(
        has_strip,
        "expected bottom strip rect, got {:?}",
        plan.rects
    );
}

#[test]
fn painted_offset_shrink_emits_positive_scroll_op_and_top_strip() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(20),
        },
    );
    world.insert(list, Dirty);
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 23);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert_eq!(plan.shifts.len(), 1);
    assert_eq!(plan.shifts[0].dy, Fixed::from_int(3));
    let has_top_strip = plan
        .rects
        .iter()
        .any(|r| r.y == Fixed::ZERO && r.h == Fixed::from_int(3) && r.w == Fixed::from_int(128));
    assert!(
        has_top_strip,
        "expected top strip rect, got {:?}",
        plan.rects
    );
}

#[test]
fn displacement_larger_than_viewport_forces_full_repaint_without_shift() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(160),
        },
    );
    world.insert(list, Dirty);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 40);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

    assert!(plan.shifts.is_empty());
    assert!(plan.rects.iter().any(|rect| *rect
        == Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(128),
            Fixed::from_int(100),
        )));
}

#[test]
fn nested_scroll_emits_inner_first() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let outer = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        outer,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
    );
    world.insert(outer, Dirty);
    let inner = spawn_widget(&mut world, Some(outer), absolute_style(0, 10, 128, 60));
    world.insert(
        inner,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
    );
    world.insert(inner, Dirty);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    painted(&mut world, root, &viewport);
    world.get_mut::<ScrollOffset>(outer).unwrap().y = Fixed::from_int(-2);
    world.get_mut::<ScrollOffset>(inner).unwrap().y = Fixed::from_int(-5);
    world.invalidate_visual(outer);
    world.invalidate_visual(inner);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert_eq!(plan.shifts.len(), 2, "two RegionShifts for two containers");
    assert_eq!(plan.shifts[0].dy, Fixed::from_int(5), "inner first");
    assert_eq!(plan.shifts[1].dy, Fixed::from_int(2), "outer second");
    assert_eq!(
        plan.shifts[0].area.y,
        Fixed::from_int(10),
        "source is painted position"
    );
    assert!(
        plan.rects
            .iter()
            .any(|rect| rect.y == Fixed::from_int(12) && rect.h == Fixed::from_int(5)),
        "exposure follows outer transfer"
    );
}

#[test]
fn scroll_plans_use_only_the_last_painted_offset() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 100.into(),
        },
    );
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    let initial = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(
        initial.shifts.is_empty(),
        "unpainted pixels cannot be transferred"
    );
    seed_prev_rects(&mut world, root, &viewport);
    let mut plan = DirtyRegions::default();
    world.get_mut::<ScrollOffset>(list).unwrap().y = 120.into();
    world.invalidate_visual(list);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

    let allocations = crate::test_support::tracked_allocations(|| {
        for offset in [120, 135, 110] {
            world.get_mut::<ScrollOffset>(list).unwrap().y = offset.into();
            for _ in 0..2 {
                world.invalidate_visual(list);
                try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
                assert_eq!(plan.shifts.len(), 1);
                assert_eq!(plan.shifts[0].dy, Fixed::from_int(100 - offset));
                assert_eq!(
                    world.get::<scroll::PaintedScroll>(list).unwrap().offset.y,
                    Fixed::from_int(100)
                );
            }
        }
        seed_prev_rects(&mut world, root, &viewport);
    });
    assert_eq!(allocations, 0);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert!(
        plan.shifts.is_empty(),
        "successful drawing acknowledges the new offset"
    );
}

#[test]
fn clipped_nested_window_is_repainted_without_discarding_outer_transfer() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    let outer = spawn_widget(&mut world, Some(root), px_style(128, 100));
    let inner = spawn_widget(&mut world, Some(outer), absolute_style(0, 0, 128, 60));
    world.insert(outer, ScrollOffset::default());
    world.insert(inner, ScrollOffset::default());
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    painted(&mut world, root, &viewport);
    world.get_mut::<ScrollOffset>(outer).unwrap().y = 2.into();
    world.get_mut::<ScrollOffset>(inner).unwrap().y = 5.into();
    world.invalidate_visual(outer);
    world.invalidate_visual(inner);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert_eq!(plan.shifts.len(), 1);
    assert_eq!(plan.shifts[0].area, Rect::new(0, 0, 128, 100));
    assert!(plan.rects.contains(&Rect::new(0, 0, 128, 58)));
}

#[test]
fn removed_scroll_offset_cannot_reuse_an_older_painted_incarnation() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(list, ScrollOffset::default());
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    painted(&mut world, root, &viewport);
    world.remove::<ScrollOffset>(list);
    world.invalidate_visual(list);
    painted(&mut world, root, &viewport);
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 5.into(),
        },
    );
    world.invalidate_visual(list);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(plan.shifts.is_empty());
    assert!(plan.rects.contains(&Rect::new(0, 0, 128, 100)));
}

#[test]
fn leaving_offscreen_or_rounded_ancestor_does_not_reuse_its_old_screen_baseline() {
    for offscreen in [false, true] {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(list, ScrollOffset::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        painted(&mut world, root, &viewport);
        if offscreen {
            world.insert(root, crate::ui::OffscreenRender::default());
        } else {
            let style = world.get_mut::<Style>(root).unwrap();
            style.clip_children = true;
            style.border_radius = 8.into();
        }
        world.invalidate_visual(root);
        painted(&mut world, root, &viewport);
        world.remove::<crate::ui::OffscreenRender>(root);
        world.get_mut::<Style>(root).unwrap().border_radius = Fixed::ZERO;
        world.get_mut::<ScrollOffset>(list).unwrap().y = 5.into();
        world.invalidate_visual(root);
        world.invalidate_visual(list);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(plan.shifts.is_empty());
    }
}

#[test]
fn unchanged_scroll_offset_emits_no_op() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
    );
    world.insert(list, Dirty);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 0);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(plan.shifts.is_empty());
}

#[test]
fn fractional_scroll_repaints_or_blits_whole_physical_pixels_without_residue() {
    for scale in [Fixed::ONE, Fixed::from_int(2)] {
        for delta in [
            Fixed::from_f32(-0.5),
            Fixed::from_f32(0.25),
            Fixed::from_f32(0.5),
        ] {
            let mut world = World::new();
            let root = spawn_widget(&mut world, None, px_style(128, 128));
            let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
            world.insert(list, ScrollOffset::default());
            let viewport = Viewport::new(128, 128, scale);
            world.insert(root, Dirty);
            painted(&mut world, root, &viewport);
            for _ in 0..3 {
                world.get_mut::<ScrollOffset>(list).unwrap().y += delta;
                world.invalidate_visual(list);
                let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
                assert!(!plan.rects.is_empty());
                if delta.abs() * scale == Fixed::ONE {
                    assert_eq!(plan.shifts.len(), 1);
                    assert_eq!(plan.shifts[0].dy, -delta);
                } else {
                    assert!(plan.shifts.is_empty());
                    let height = Fixed::from_int(i32::from(viewport.logical_size().1).min(100));
                    assert!(plan.rects.iter().any(|rect| rect.h == height));
                }
                seed_prev_rects(&mut world, root, &viewport);
            }
        }
    }
}

fn absolute_style(left: i32, top: i32, w: i32, h: i32) -> Style {
    use crate::ui::layout::Position;
    Style {
        layout: LayoutStyle {
            position: Position::Absolute,
            left: Dimension::Px(Fixed::from_int(left)),
            top: Dimension::Px(Fixed::from_int(top)),
            width: Dimension::Px(Fixed::from_int(w)),
            height: Dimension::Px(Fixed::from_int(h)),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn overlay_over_scroll_covers_both_positions_without_overlapping_draws() {
    use crate::input::feedback::OverlayCursor;

    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let list = spawn_widget(&mut world, Some(root), absolute_style(0, 0, 128, 100));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(2),
        },
    );
    world.insert(list, Dirty);

    let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 50, 40, 12));
    world.insert(overlay, OverlayCursor);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 0);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

    assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
    let dy = plan.shifts[0].dy;
    assert_eq!(dy, Fixed::from_int(-2));

    assert_eq!(
        plan.rects,
        [Rect::new(40, 48, 40, 14), Rect::new(0, 98, 128, 2)]
    );
}

#[test]
fn overlay_outside_scroll_container_keeps_scroll_op() {
    use crate::input::feedback::OverlayCursor;

    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(128, 128));
    world.insert(root, Dirty);
    let list = spawn_widget(&mut world, Some(root), absolute_style(0, 30, 128, 90));
    world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(2),
        },
    );
    world.insert(list, Dirty);

    let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 0, 40, 12));
    world.insert(overlay, OverlayCursor);

    let viewport = Viewport::new(128, 128, Fixed::ONE);
    move_from_painted(&mut world, root, &viewport, list, 0);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
}

#[test]
fn dirty_walker_publishes_layout_snapshot_for_render_walker() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(64, 64));
    let child = spawn_widget(&mut world, Some(root), px_style(32, 32));
    world.insert(child, Dirty);

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(!plan.rects.is_empty(), "child Dirty should produce a rect");

    let snap = world
        .resource::<LayoutSnapshot>()
        .expect("dirty walker must publish a LayoutSnapshot");
    assert_eq!(
        snap.entities,
        std::vec![root, child],
        "snapshot must enumerate the same preorder as render walker would build",
    );
    assert_eq!(snap.layout_tree.children.len(), 1);
}

#[test]
fn first_pass_publishes_input_geometry_and_preserves_initial_damage() {
    let mut world = World::new();
    let root = spawn_widget(&mut world, None, px_style(64, 64));
    spawn_widget(&mut world, Some(root), px_style(32, 32));

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(!plan.rects.is_empty());
    assert!(world.resource::<LayoutSnapshot>().is_some());
    assert!(crate::input::event::hit_test::geometry_matches(
        &world, root, 64, 64,
    ));
}

#[test]
fn render_region_cached_matches_fresh_output() {
    use crate::render::command::DrawCommand;
    use crate::render::renderer::Renderer;

    struct Recorder(std::vec::Vec<core::mem::Discriminant<DrawCommand<'static>>>);
    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            let static_cmd: &DrawCommand<'static> =
                unsafe { core::mem::transmute(request.command) };
            self.0.push(core::mem::discriminant(static_cmd));
            Ok(())
        }
        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let styled = |w, h| Style {
        layout: LayoutStyle {
            width: Dimension::Px(Fixed::from_int(w)),
            height: Dimension::Px(Fixed::from_int(h)),
            ..Default::default()
        },
        bg_color: Some(crate::ui::theme::ThemedColor::Raw(
            crate::types::Color::rgb(255, 0, 0),
        )),
        ..Default::default()
    };

    let mut world = World::new();
    world.insert_resource(crate::ui::view::ViewRegistry::with_builtins());
    world.insert_resource(crate::ui::theme::Theme::default());
    let root = spawn_widget(&mut world, None, styled(64, 64));
    let child = spawn_widget(&mut world, Some(root), styled(32, 32));
    world.insert(child, Dirty);

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let dirty_rect = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(64),
        Fixed::from_int(64),
    );
    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert!(!plan.rects.is_empty());

    let snap = world
        .resource::<LayoutSnapshot>()
        .expect("snapshot present after dirty walk");
    let mut cached_out = Recorder(std::vec::Vec::new());
    let requests = prepare_paint_requests(&world).unwrap();
    render_region_cached(&world, snap, &dirty_rect, &mut cached_out, &requests, None).unwrap();
    requests.commit();

    let mut fresh_out = Recorder(std::vec::Vec::new());
    render_region(&world, root, &viewport, &dirty_rect, &mut fresh_out).unwrap();

    assert_eq!(
        cached_out.0, fresh_out.0,
        "cached and fresh paths must emit the same DrawCommand sequence",
    );
    assert!(
        !cached_out.0.is_empty(),
        "test must exercise at least one DrawCommand",
    );
}
