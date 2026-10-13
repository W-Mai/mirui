use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Transform;
use crate::ui::widgets::{DropShadow, MirrorOf, WidgetTransform};
use crate::ui::{OffscreenRender, Outputs, WidgetTexture};

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

const SIZE: usize = 24;
const BLACK: Color = Color::rgb(0, 0, 0);
const WHITE: Color = Color::rgb(255, 255, 255);
const RED: Color = Color::rgb(255, 0, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
const GREEN: Color = Color::rgb(0, 255, 0);
const YELLOW: Color = Color::rgb(255, 255, 0);

fn app() -> (TestApp, Entity) {
    let mut app = App::headless(SIZE as u16, SIZE as u16);
    app.with_default_widgets().with_offscreen_pool_budget(4096);
    let root = app.spawn_root().bg_color(BLACK).id();
    (app, root)
}

fn pixel(app: &mut TestApp, x: i32, y: i32) -> Color {
    app.backend.framebuffer().get_pixel(x, y)
}

fn pixels(app: &mut TestApp) -> [Color; SIZE * SIZE] {
    let target = app.backend.framebuffer();
    core::array::from_fn(|index| target.get_pixel((index % SIZE) as i32, (index / SIZE) as i32))
}

fn assert_same_pixels(actual: &mut TestApp, reference: &mut TestApp) {
    let actual = pixels(actual);
    let reference = pixels(reference);
    for (index, (actual, reference)) in actual.into_iter().zip(reference).enumerate() {
        assert_eq!(
            actual,
            reference,
            "pixel ({}, {})",
            index % SIZE,
            index / SIZE
        );
    }
}

fn set_inline(app: &mut TestApp, capture: Entity) {
    assert!(app.world.remove::<OffscreenRender>(capture).is_some());
}

#[crate::compose]
fn overflow_scene() {
    ui! {
        View (
            id: "capture",
            position: Position::Absolute,
            left: 8,
            top: 4,
            width: 4,
            height: 4,
            bg_color: GREEN,
            border_color: WHITE,
            border_width: 1
        ) [
            OffscreenRender::new(),
        ] {
            View (
                id: "left",
                position: Position::Absolute,
                left: -4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "right",
                position: Position::Absolute,
                left: 4,
                top: 2,
                width: 4,
                height: 4,
                bg_color: BLUE
            )
        }
    };
}

fn overflow_app(captured: bool) -> (TestApp, Entity) {
    let (mut app, root) = app();
    app.compose(root, overflow_scene());
    let capture = app.world.find_by_id("capture").unwrap();
    if !captured {
        set_inline(&mut app, capture);
    }
    (app, capture)
}

#[test]
fn captured_overflow_and_border_match_inline_pixels_and_clear_old_child_extent() {
    let (mut captured, capture) = overflow_app(true);
    let (mut reference, _) = overflow_app(false);
    captured.render().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    assert_eq!(pixel(&mut captured, 5, 5), RED);
    assert_eq!(pixel(&mut captured, 8, 4), WHITE);
    assert_eq!(pixel(&mut captured, 15, 9), BLUE);
    {
        let outputs = captured.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(capture).unwrap();
        let bounds = output.local_bounds();
        assert!(bounds.x <= Fixed::from_int(-4));
        assert!(bounds.x + bounds.w >= Fixed::from_int(8));
        assert!(bounds.y + bounds.h >= Fixed::from_int(6));
    }

    for app in [&mut captured, &mut reference] {
        let right = app.world.find_by_id("right").unwrap();
        let style = app.world.get_mut::<Style>(right).unwrap();
        style.layout.left = Dimension::Px(Fixed::from_int(6));
        style.layout.width = Dimension::Px(Fixed::from_int(2));
        style.layout.height = Dimension::Px(Fixed::from_int(2));
    }
    captured.render_dirty().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    assert_eq!(pixel(&mut captured, 13, 7), BLACK);
    assert_eq!(pixel(&mut captured, 15, 9), BLACK);
    assert_eq!(pixel(&mut captured, 15, 7), BLUE);
}

#[crate::compose]
fn external_shadow_scene() {
    ui! {
        View (width: 24, height: 24) {
            View (
                id: "source",
                position: Position::Absolute,
                left: 10,
                top: 8,
                width: 4,
                height: 4,
                bg_color: WHITE
            )
            View (
                id: "capture",
                position: Position::Absolute,
                left: 2,
                top: 2,
                width: 2,
                height: 2
            ) [
                OffscreenRender::new(),
                DropShadow::new(id("source"))
                    .with_blur_radius(1)
                    .with_offset(2, 0)
                    .with_color(RED)
                    .with_opacity(255),
            ]
        }
    };
}

fn shadow_app(captured: bool) -> TestApp {
    let (mut app, root) = app();
    app.compose(root, external_shadow_scene());
    if !captured {
        let capture = app.world.find_by_id("capture").unwrap();
        set_inline(&mut app, capture);
    }
    app
}

#[test]
fn captured_shadow_outside_owner_layout_matches_inline_and_erases_moved_shrunk_halo() {
    let mut captured = shadow_app(true);
    let mut reference = shadow_app(false);
    captured.render().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    let halo = pixel(&mut captured, 15, 9);
    assert!(halo.r > 0);
    assert_eq!((halo.g, halo.b), (0, 0));

    for app in [&mut captured, &mut reference] {
        let source = app.world.find_by_id("source").unwrap();
        let style = app.world.get_mut::<Style>(source).unwrap();
        style.layout.left = Dimension::Px(Fixed::from_int(16));
        style.layout.top = Dimension::Px(Fixed::from_int(12));
        style.layout.width = Dimension::Px(Fixed::from_int(2));
        style.layout.height = Dimension::Px(Fixed::from_int(2));
    }
    captured.render_dirty().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    assert_eq!(pixel(&mut captured, 15, 9), BLACK);
    let halo = pixel(&mut captured, 19, 13);
    assert!(halo.r > 0);
    assert_eq!((halo.g, halo.b), (0, 0));
}

#[crate::compose]
fn clipped_capture_scene(clips: bool) {
    ui! {
        View (
            id: "capture",
            position: Position::Absolute,
            left: 4,
            top: 4,
            width: 4,
            height: 4,
            clip_children: clips
        ) [
            OffscreenRender::new(),
        ] {
            View (
                id: "first",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "second",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: BLUE
            )
        }
    };
}

fn clipping_app(captured: bool, scrolls: bool) -> (TestApp, Entity) {
    let (mut app, root) = app();
    app.compose(root, clipped_capture_scene(!scrolls));
    let capture = app.world.find_by_id("capture").unwrap();
    if scrolls {
        app.world.insert(
            capture,
            ScrollOffset {
                x: Fixed::from_int(2),
                y: Fixed::ZERO,
            },
        );
    }
    if !captured {
        set_inline(&mut app, capture);
    }
    (app, capture)
}

#[test]
fn expanded_captures_preserve_explicit_child_clips_and_scrolling_viewports() {
    for scrolls in [false, true] {
        let (mut captured, capture) = clipping_app(true, scrolls);
        let (mut reference, _) = clipping_app(false, scrolls);
        captured.render().unwrap();
        reference.render().unwrap();
        assert_same_pixels(&mut captured, &mut reference);
        assert_eq!(pixel(&mut captured, 5, 5), RED);
        assert_eq!(pixel(&mut captured, 7, 5), if scrolls { BLUE } else { RED });
        assert_eq!(pixel(&mut captured, 3, 5), BLACK);
        assert_eq!(pixel(&mut captured, 9, 5), BLACK);
        assert_eq!(
            captured
                .world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(capture)
                .unwrap()
                .local_bounds(),
            Rect::new(0, 0, 4, 4)
        );

        for app in [&mut captured, &mut reference] {
            for (name, color) in [("first", GREEN), ("second", YELLOW)] {
                let entity = app.world.find_by_id(name).unwrap();
                app.world.get_mut::<Style>(entity).unwrap().bg_color = Some(color.into());
            }
        }
        captured.render_dirty().unwrap();
        reference.render().unwrap();
        assert_same_pixels(&mut captured, &mut reference);
        assert_eq!(pixel(&mut captured, 5, 5), GREEN);
        assert_eq!(
            pixel(&mut captured, 7, 5),
            if scrolls { YELLOW } else { GREEN }
        );
        assert_eq!(pixel(&mut captured, 3, 5), BLACK);
        assert_eq!(pixel(&mut captured, 9, 5), BLACK);
    }
}

#[crate::compose]
fn fractional_capture_scene() {
    ui! {
        View (
            id: "capture",
            position: Position::Absolute,
            left: Fixed::from_ratio(17, 4),
            top: Fixed::from_ratio(7, 2),
            width: Fixed::from_ratio(7, 2),
            height: Fixed::from_ratio(5, 2),
            bg_color: RED
        ) [
            OffscreenRender::with_scale(Fixed::HALF),
        ]
    };
}

#[test]
fn fractional_capture_dimensions_round_outward_after_applying_output_scale() {
    let (mut app, root) = app();
    app.compose(root, fractional_capture_scene());
    app.render().unwrap();
    let capture = app.world.find_by_id("capture").unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(capture).unwrap();
        assert!(output.texture().width >= 2);
        assert!(output.texture().height >= 2);
        let bounds = output.local_bounds();
        assert!(bounds.x <= Fixed::ZERO);
        assert!(bounds.y <= Fixed::ZERO);
        assert!(bounds.x + bounds.w >= Fixed::from_ratio(7, 2));
        assert!(bounds.y + bounds.h >= Fixed::from_ratio(5, 2));
    }
    let inside = pixel(&mut app, 6, 4);
    assert!(inside.r > 0);
    assert_eq!((inside.g, inside.b), (0, 0));
    assert_eq!(pixel(&mut app, 1, 1), BLACK);
}

#[crate::compose]
fn rotated_capture_scene() {
    ui! {
        View (
            id: "capture",
            position: Position::Absolute,
            left: 8,
            top: 6,
            width: 4,
            height: 4,
            bg_color: GREEN
        ) [
            OffscreenRender::new(),
            WidgetTransform(Transform {
                m00: Fixed::ZERO,
                m01: Fixed::from_int(-1),
                tx: Fixed::ZERO,
                m10: Fixed::ONE,
                m11: Fixed::ZERO,
                ty: Fixed::ZERO,
            }),
        ] {
            View (
                id: "child",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
        }
    };
}

fn rotated_app(captured: bool) -> TestApp {
    let (mut app, root) = app();
    app.compose(root, rotated_capture_scene());
    if !captured {
        let capture = app.world.find_by_id("capture").unwrap();
        set_inline(&mut app, capture);
    }
    app
}

#[test]
fn changing_capture_overflow_does_not_move_the_layout_rotation_pivot() {
    let mut captured = rotated_app(true);
    let mut reference = rotated_app(false);
    captured.render().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    assert_eq!(pixel(&mut captured, 9, 7), GREEN);
    assert_eq!(pixel(&mut captured, 9, 11), RED);

    for app in [&mut captured, &mut reference] {
        let child = app.world.find_by_id("child").unwrap();
        app.world.get_mut::<Style>(child).unwrap().layout.left = Dimension::Px(Fixed::from_int(8));
    }
    captured.render_dirty().unwrap();
    reference.render().unwrap();
    assert_same_pixels(&mut captured, &mut reference);
    assert_eq!(pixel(&mut captured, 9, 7), GREEN);
    assert_eq!(pixel(&mut captured, 9, 11), BLACK);
    assert_eq!(pixel(&mut captured, 9, 15), RED);
}

#[crate::compose]
fn retained_resize_scene() {
    ui! {
        View (width: 24, height: 24) {
            View (
                id: "retained",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            ) [
                OffscreenRender::new(),
            ]
            View (
                id: "replacement",
                position: Position::Absolute,
                left: 8,
                top: 0,
                width: 0,
                height: 4,
                bg_color: GREEN
            ) [
                OffscreenRender::new(),
            ]
        }
    };
}

#[test]
fn zero_capture_geometry_releases_both_history_planes_before_other_outputs_are_admitted() {
    let (mut app, root) = app();
    app.with_offscreen_pool_budget(2 * 4 * 4 * 4);
    app.compose(root, retained_resize_scene());
    let retained = app.world.find_by_id("retained").unwrap();
    let replacement = app.world.find_by_id("replacement").unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        outputs.reserve(1).unwrap();
        outputs.set_retained(retained, true).unwrap();
    }
    app.render().unwrap();
    app.world.get_mut::<Style>(retained).unwrap().bg_color = Some(BLUE.into());
    app.render_dirty().unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert_eq!(
            outputs.get(retained).unwrap().texture().get_pixel(1, 1),
            BLUE
        );
        assert_eq!(
            outputs
                .previous(retained)
                .unwrap()
                .texture()
                .get_pixel(1, 1),
            RED
        );
    }

    app.world.get_mut::<Style>(retained).unwrap().layout.width = Dimension::Px(Fixed::ZERO);
    app.world
        .get_mut::<Style>(replacement)
        .unwrap()
        .layout
        .width = Dimension::Px(Fixed::from_int(8));
    app.render_dirty().unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(outputs.get(retained).is_none());
        assert!(outputs.previous(retained).is_none());
        let replacement = outputs.get(replacement).unwrap();
        assert_eq!(
            (replacement.texture().width, replacement.texture().height),
            (8, 4)
        );
        assert_eq!(replacement.texture().get_pixel(7, 1), GREEN);
    }
    assert!(app.world.is_alive(retained));
    assert_eq!(pixel(&mut app, 1, 1), BLACK);
    assert_eq!(pixel(&mut app, 15, 1), GREEN);

    // Keep both actors alive; the replacement stops reserving output pixels.
    assert!(app.world.remove::<OffscreenRender>(replacement).is_some());
    {
        let style = app.world.get_mut::<Style>(retained).unwrap();
        style.layout.width = Dimension::Px(Fixed::from_int(4));
        style.bg_color = Some(YELLOW.into());
    }
    app.render_dirty().unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert_eq!(
        outputs.get(retained).unwrap().texture().get_pixel(1, 1),
        YELLOW
    );
    assert!(outputs.previous(retained).is_none());
    assert!(outputs.state(retained).unwrap().retained);
    assert!(app.world.is_alive(replacement));
}

#[crate::compose]
fn narrow_capture_scene(width: Fixed) {
    ui! {
        View (
            id: "capture",
            position: Position::Absolute,
            left: 4,
            top: 4,
            width: width,
            height: 4,
            bg_color: RED
        ) [
            OffscreenRender::with_scale(Fixed::from_int(4)),
        ]
    };
}

#[test]
fn fractional_capture_blits_preserve_subpixel_width_and_the_last_covered_column() {
    for quarter_pixels in [3, 5, 7] {
        let width = Fixed::from_ratio(quarter_pixels, 4);
        let (mut captured, root) = app();
        captured.compose(root, narrow_capture_scene(width));
        let capture = captured.world.find_by_id("capture").unwrap();
        let (mut reference, root) = app();
        reference.compose(root, narrow_capture_scene(width));
        let reference_capture = reference.world.find_by_id("capture").unwrap();
        set_inline(&mut reference, reference_capture);
        captured.render().unwrap();
        reference.render().unwrap();
        {
            let outputs = captured.world.resource::<Outputs<WidgetTexture>>().unwrap();
            let texture = outputs.get(capture).unwrap();
            assert_eq!(texture.texture().width, quarter_pixels as u16);
            assert_eq!(texture.texture().height, 16);
        }

        // Binary quad coverage may differ from fractional rectangle coverage at the edge.
        let edge_tolerance = if cfg!(feature = "quad-aa") { 2 } else { 64 };
        for y in [5, 6] {
            for x in 0..SIZE as i32 {
                let actual = pixel(&mut captured, x, y);
                let expected = pixel(&mut reference, x, y);
                let edge = if quarter_pixels < 4 { x == 4 } else { x == 5 };
                let tolerance = if edge { edge_tolerance } else { 0 };
                assert!(
                    actual.r.abs_diff(expected.r) <= tolerance,
                    "width {width:?}, pixel ({x}, {y}): {actual:?} != {expected:?}"
                );
                assert_eq!(
                    (actual.g, actual.b, actual.a),
                    (expected.g, expected.b, expected.a)
                );
            }
            assert!(pixel(&mut captured, 4, y).r > 0);
            if quarter_pixels == 7 {
                assert!(pixel(&mut captured, 5, y).r > 0);
            }
        }
    }
}

#[crate::compose]
fn partially_missing_mirror_capture() {
    ui! {
        View (width: 24, height: 24) {
            View (
                id: "missing_source",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "healthy_source",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: GREEN
            )
            View (
                id: "capture",
                position: Position::Absolute,
                left: 0,
                top: 8,
                width: 8,
                height: 4
            ) [
                OffscreenRender::new(),
            ] {
                View (position: Position::Absolute, left: 0, top: 0, width: 4, height: 4) [
                    MirrorOf::new(id("missing_source")),
                ]
                View (position: Position::Absolute, left: 4, top: 0, width: 4, height: 4) [
                    MirrorOf::new(id("healthy_source")),
                ]
            }
        }
    };
}

#[test]
fn missing_capture_dependency_preserves_workspace_for_healthy_inline_fallback() {
    let (mut app, root) = app();
    app.compose(root, partially_missing_mirror_capture());
    let missing = app.world.find_by_id("missing_source").unwrap();
    let healthy = app.world.find_by_id("healthy_source").unwrap();
    let capture = app.world.find_by_id("capture").unwrap();
    // A live, visible source outside this layout is unavailable to the capture plan.
    let parent = app.world.get::<crate::ui::Parent>(missing).unwrap().0;
    app.world
        .get_mut::<crate::ui::Children>(parent)
        .unwrap()
        .0
        .retain(|entity| *entity != missing);
    assert!(app.world.remove::<crate::ui::Parent>(missing).is_some());
    assert!(app.world.is_alive(missing));
    assert!(!app.world.has::<crate::ui::Hidden>(missing));
    app.render().unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(outputs.get(missing).is_none());
        assert!(outputs.get(capture).is_none());
        assert_eq!(
            outputs.get(healthy).unwrap().texture().get_pixel(1, 1),
            GREEN
        );
    }
    assert_eq!(pixel(&mut app, 1, 9), BLACK);
    assert_eq!(pixel(&mut app, 5, 9), GREEN);

    app.world.get_mut::<Style>(healthy).unwrap().bg_color = Some(BLUE.into());
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(capture)
            .is_none()
    );
    assert_eq!(pixel(&mut app, 1, 9), BLACK);
    assert_eq!(pixel(&mut app, 5, 9), BLUE);
}

#[crate::compose]
fn seed_plain_scene() {
    ui! {
        View (
            id: "seed_plain",
            position: Position::Absolute,
            left: 2,
            top: 16,
            width: 2,
            height: 2,
            bg_color: WHITE
        )
    };
}

#[test]
fn seed_after_root_change_does_not_apply_another_roots_capture_geometry() {
    use crate::ui::render_system::{render, seed_prev_rects, try_update_layout};

    for retain_snapshot in [false, true] {
        let (mut app, old_capture) = overflow_app(true);
        app.render().unwrap();
        assert!(
            app.world
                .get::<crate::ui::dirty::PrevRect>(old_capture)
                .unwrap()
                .0
                .w
                > Fixed::from_int(4)
        );
        let root = app.spawn_root().bg_color(BLACK).id();
        app.compose(root, seed_plain_scene());
        let plain = app.world.find_by_id("seed_plain").unwrap();
        let viewport = crate::types::Viewport::new(SIZE as u16, SIZE as u16, Fixed::ONE);
        if retain_snapshot {
            try_update_layout(&mut app.world, root, &viewport).unwrap();
        }
        seed_prev_rects(&mut app.world, root, &viewport);
        assert_eq!(
            app.world
                .get::<crate::ui::dirty::PrevRect>(plain)
                .unwrap()
                .0,
            Rect::new(2, 16, 2, 2),
            "retained snapshot: {retain_snapshot}"
        );
    }

    let (mut app, retained_root) = app();
    app.compose(retained_root, seed_plain_scene());
    let plain = app.world.find_by_id("seed_plain").unwrap();
    app.render().unwrap();
    let temporary_root = app.spawn_root().bg_color(BLACK).id();
    app.compose(temporary_root, overflow_scene());
    let viewport = crate::types::Viewport::new(SIZE as u16, SIZE as u16, Fixed::ONE);
    {
        // Exercise the low-level temporary tree without replacing the retained snapshot.
        let mut renderer = crate::render::sw::SwRenderer::new(app.backend.framebuffer());
        render(&app.world, temporary_root, &viewport, &mut renderer).unwrap();
    }
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        assert!(plan.matches_root(temporary_root));
        assert!(!plan.captures.is_empty());
        outputs.put_texture_plan(plan);
    }
    seed_prev_rects(&mut app.world, retained_root, &viewport);
    assert_eq!(
        app.world
            .get::<crate::ui::dirty::PrevRect>(plain)
            .unwrap()
            .0,
        Rect::new(2, 16, 2, 2)
    );
}

#[test]
fn layout_reorder_invalidates_seed_geometry_and_reuses_plan_capacity() {
    use crate::ui::render_system::{seed_prev_rects, try_update_layout};

    let (mut app, capture) = overflow_app(true);
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    app.compose(root, seed_plain_scene());
    let plain = app.world.find_by_id("seed_plain").unwrap();
    app.render().unwrap();
    let capacities = {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        assert!(!plan.nodes.is_empty());
        assert!(!plan.captures.is_empty());
        let capacities = (plan.nodes.capacity(), plan.captures.capacity());
        outputs.put_texture_plan(plan);
        capacities
    };
    {
        let children = &mut app.world.get_mut::<crate::ui::Children>(root).unwrap().0;
        assert_eq!(children.as_slice(), &[capture, plain]);
        children.swap(0, 1);
    }
    let viewport = crate::types::Viewport::new(SIZE as u16, SIZE as u16, Fixed::ONE);
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        assert!(plan.nodes.is_empty());
        assert!(plan.captures.is_empty());
        assert_eq!(plan.root, None);
        assert!(!plan.geometry_ready);
        assert_eq!(
            (plan.nodes.capacity(), plan.captures.capacity()),
            capacities
        );
        outputs.put_texture_plan(plan);
    }
    seed_prev_rects(&mut app.world, root, &viewport);
    assert_eq!(
        app.world
            .get::<crate::ui::dirty::PrevRect>(plain)
            .unwrap()
            .0,
        Rect::new(2, 16, 2, 2)
    );

    app.render().unwrap();
    let capture_rect = app.world.get::<crate::ui::ComputedRect>(capture).unwrap().0;
    let previous = app
        .world
        .get::<crate::ui::dirty::PrevRect>(capture)
        .unwrap()
        .0;
    assert!(previous.w > capture_rect.w);
    assert_eq!(pixel(&mut app, 5, 5), RED);
}
