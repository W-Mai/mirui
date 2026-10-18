use crate::ecs::World;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::widgets::{DropGlow, DropShadow, MirrorOf};
use crate::ui::{OffscreenRender, Outputs, WidgetTexture};

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

const BLACK: Color = Color::rgb(0, 0, 0);
const WHITE: Color = Color::rgb(255, 255, 255);
const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
// Source, enclosing output, independent output, and the largest shared plane.
const BUDGET: usize = 4 * 4 * 4 + 16 * 16 * 4 + 2 * 2 * 4 + 8 * 8 * 4;

#[crate::compose]
fn effect_scene(origin_x: i32, origin_y: i32) {
    ui! {
        View (width: 24, height: 24) {
            View (
                id: "capture",
                position: Position::Absolute,
                left: origin_x,
                top: origin_y,
                width: 16,
                height: 16
            ) [
                OffscreenRender::new(),
            ] {
                View (
                    id: "source",
                    position: Position::Absolute,
                    left: 6,
                    top: 6,
                    width: 4,
                    height: 4,
                    bg_color: WHITE
                )
                View (
                    id: "mirror",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 4,
                    height: 4
                ) [
                    MirrorOf::new(id("source")),
                ]
                View (
                    id: "shadow",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 16,
                    height: 16
                ) [
                    DropShadow::new(id("source"))
                        .with_blur_radius(1)
                        .with_offset(1, 1)
                        .with_color(RED)
                        .with_opacity(255),
                ]
                View (
                    id: "glow",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 16,
                    height: 16
                ) [
                    DropGlow::new(id("source")).with_blur_radius(2).with_color(GREEN).with_opacity(255),
                ]
            }
            View (
                id: "independent",
                position: Position::Absolute,
                left: 22,
                top: 0,
                width: 2,
                height: 2,
                bg_color: BLUE
            ) [
                OffscreenRender::new(),
            ]
        }
    };
}

fn test_app(origin: (i32, i32), budget: usize) -> TestApp {
    let mut app = App::headless(24, 24);
    app.with_default_widgets()
        .with_offscreen_pool_budget(budget);
    let root = app.spawn_root().bg_color(BLACK).id();
    app.compose(root, effect_scene(origin.0, origin.1));
    app
}

fn capture_pixels(world: &World) -> [Color; 16 * 16] {
    let capture = world.find_by_id("capture").unwrap();
    let output = world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(capture)
        .unwrap();
    let mut pixels = [BLACK; 16 * 16];
    for y in 0..16 {
        for x in 0..16 {
            pixels[y * 16 + x] = output
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(x as i32, y as i32);
        }
    }
    pixels
}

#[cfg(feature = "std")]
fn change_alpha_and_opacity(world: &mut World, bright: bool) {
    let source = world.find_by_id("source").unwrap();
    let shadow = world.find_by_id("shadow").unwrap();
    let glow = world.find_by_id("glow").unwrap();
    let (alpha, opacity) = if bright { (255, 255) } else { (96, 128) };
    world.get_mut::<Style>(source).unwrap().bg_color =
        Some(Color::rgba(255, 255, 255, alpha).into());
    world.get_mut::<DropShadow>(shadow).unwrap().opacity = opacity;
    world.get_mut::<DropGlow>(glow).unwrap().opacity = opacity;
}

#[cfg(feature = "std")]
#[test]
fn changing_drop_effect_alpha_and_opacity_repaints_without_warm_allocations() {
    let mut app = test_app((0, 0), BUDGET);
    app.render().unwrap();
    let bright = capture_pixels(&app.world);
    let bright_halo = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .get_pixel(10, 7);
    change_alpha_and_opacity(&mut app.world, false);
    app.render_dirty().unwrap();
    let dim = capture_pixels(&app.world);
    let dim_halo = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .get_pixel(10, 7);
    assert_ne!(bright, dim);
    assert!(bright_halo.r > dim_halo.r);
    assert!(bright_halo.g > dim_halo.g);
    change_alpha_and_opacity(&mut app.world, true);
    app.render_dirty().unwrap();
    assert_eq!(capture_pixels(&app.world), bright);
    change_alpha_and_opacity(&mut app.world, false);
    app.render_dirty().unwrap();

    let capture = app.world.find_by_id("capture").unwrap();
    let mut version = app
        .world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(capture)
        .unwrap()
        .texture()
        .cache_revision;
    let allocations = crate::test_support::tracked_allocations(|| {
        for frame in 0..32 {
            let is_bright = frame % 2 == 0;
            change_alpha_and_opacity(&mut app.world, is_bright);
            app.render_dirty().unwrap();
            assert_eq!(
                capture_pixels(&app.world),
                if is_bright { bright } else { dim }
            );
            let current = app
                .world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(capture)
                .unwrap()
                .texture()
                .cache_revision;
            assert!(current > version);
            version = current;
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn mirror_shadow_and_glow_share_the_exact_peak_budget_and_clear_reused_pixels() {
    let mut app = test_app((0, 0), BUDGET);
    app.render().unwrap();
    let pixels = capture_pixels(&app.world);
    assert_eq!(pixels[17], WHITE);
    assert!(pixels[7 * 16 + 10].r > 0);
    assert!(pixels[7 * 16 + 10].g > 0);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(22, 0),
        BLUE
    );

    let source = app.world.find_by_id("source").unwrap();
    app.world.get_mut::<Style>(source).unwrap().bg_color =
        Some(Color::rgba(255, 255, 255, 0).into());
    app.render_dirty().unwrap();
    assert!(capture_pixels(&app.world).iter().all(|pixel| pixel.a == 0));
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(10, 7),
        BLACK
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(22, 0),
        BLUE
    );
}

#[test]
fn rejected_drop_workspace_keeps_sources_and_independent_outputs_available() {
    let mut app = test_app((0, 0), BUDGET - 1);
    app.render().unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let capture = app.world.find_by_id("capture").unwrap();
    assert!(outputs.get(capture).is_none());
    for (name, expected) in [("source", WHITE), ("independent", BLUE)] {
        let entity = app.world.find_by_id(name).unwrap();
        assert_eq!(
            outputs
                .get(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            expected
        );
    }
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(10, 7),
        BLACK
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(22, 0),
        BLUE
    );
}

#[test]
fn nonzero_capture_origin_is_removed_once_from_source_anchored_drop_effects() {
    let mut reference = test_app((0, 0), BUDGET);
    let mut shifted = test_app((4, 4), BUDGET);
    reference.render().unwrap();
    shifted.render().unwrap();
    assert_eq!(
        capture_pixels(&shifted.world),
        capture_pixels(&reference.world)
    );
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(
                shifted
                    .backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(x + 4, y + 4),
                reference
                    .backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(x, y),
            );
        }
    }
}

#[test]
fn source_position_and_size_refresh_cached_inline_and_captured_measurements() {
    for captured in [false, true] {
        let mut app = test_app((0, 0), BUDGET * 2);
        let capture = app.world.find_by_id("capture").unwrap();
        if !captured {
            app.world.remove::<OffscreenRender>(capture);
        }
        let source = app.world.find_by_id("source").unwrap();
        let shadow = app.world.find_by_id("shadow").unwrap();
        let glow = app.world.find_by_id("glow").unwrap();
        let mirror = app.world.find_by_id("mirror").unwrap();
        app.render().unwrap();
        let snapshot = app.world.resource::<super::LayoutSnapshot>().unwrap();
        let plan = super::measure_for_dirty(&app.world, &snapshot.layout_tree, &snapshot.entities);
        super::return_plan(&app.world, plan);
        let measured = |app: &TestApp| {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            let snapshot = app.world.resource::<super::LayoutSnapshot>().unwrap();
            let plan = outputs.take_texture_plan().unwrap();
            let index = snapshot
                .entities
                .iter()
                .position(|&entity| entity == shadow)
                .unwrap();
            let node = &plan.nodes[index];
            assert!(node.measurement_valid);
            let result = (node.extra.unwrap(), node.workspace_bytes);
            outputs.put_texture_plan(plan);
            result
        };
        let original = measured(&app);
        for (left, top, width) in [(8, 7, 4), (8, 7, 5)] {
            let style = app.world.get_mut::<Style>(source).unwrap();
            style.layout.left = Dimension::px(left);
            style.layout.top = Dimension::px(top);
            style.layout.width = Dimension::px(width);
            let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
            crate::ui::render_system::try_update_layout(
                &mut app.world,
                root,
                &crate::types::Viewport::new(24, 24, Fixed::ONE),
            )
            .unwrap();
            for consumer in [shadow, glow, mirror] {
                crate::ui::dirty::VisualDirty::prepare(&mut app.world, consumer);
            }
            let snapshot = app.world.resource::<super::LayoutSnapshot>().unwrap();
            let plan =
                super::measure_for_dirty(&app.world, &snapshot.layout_tree, &snapshot.entities);
            super::return_plan(&app.world, plan);
            let current = measured(&app);
            assert_eq!(current.0.x, original.0.x + Fixed::from_int(2));
            assert_eq!(current.0.y, original.0.y + Fixed::ONE);
            assert_eq!(current.0.w, original.0.w + Fixed::from_int(width - 4));
            if width > 4 {
                assert!(current.1 > original.1);
            }
            app.render_dirty().unwrap();
            let partial = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            assert_eq!(
                app.backend.framebuffer().read_pixels().unwrap().bytes(),
                partial
            );
        }
    }
}

#[test]
fn half_resolution_source_preserves_the_shadows_logical_coverage() {
    let mut app = test_app((0, 0), BUDGET);
    let source = app.world.find_by_id("source").unwrap();
    let glow = app.world.find_by_id("glow").unwrap();
    app.world
        .insert(source, OffscreenRender::with_scale(Fixed::HALF));
    app.world.get_mut::<DropGlow>(glow).unwrap().opacity = 0;
    app.render().unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let source_output = outputs.get(source).unwrap();
    assert_eq!(
        (
            source_output.texture().width,
            source_output.texture().height
        ),
        (2, 2)
    );
    assert_eq!(source_output.local_bounds(), Rect::new(0, 0, 4, 4));
    let pixel = capture_pixels(&app.world)[8 * 16 + 10];
    assert!(
        pixel.a > 64 && pixel.a < 255,
        "shadow must cover the full logical source width: {pixel:?}"
    );
    assert_eq!(pixel.r, 255);
    assert_eq!(pixel.g, 0);
    assert_eq!(pixel.b, 0);
}

#[test]
fn oversized_radius_and_offsets_are_rejected_without_poisoning_other_outputs() {
    let mut app = test_app((0, 0), BUDGET);
    let shadow = app.world.find_by_id("shadow").unwrap();
    let glow = app.world.find_by_id("glow").unwrap();
    app.world.get_mut::<DropShadow>(shadow).unwrap().blur_radius = Fixed::MAX;
    app.world.get_mut::<DropGlow>(glow).unwrap().blur_radius = Fixed::MAX;
    app.render().unwrap();
    assert_eq!(capture_pixels(&app.world)[7 * 16 + 10].a, 0);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(22, 0),
        BLUE
    );

    let effect = app.world.get_mut::<DropShadow>(shadow).unwrap();
    effect.blur_radius = Fixed::ONE;
    effect.offset = (Fixed::MAX, Fixed::MIN);
    app.render_dirty().unwrap();
    assert_eq!(capture_pixels(&app.world)[7 * 16 + 10].a, 0);
    let independent = app.world.find_by_id("independent").unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(independent)
            .is_some()
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(22, 0),
        BLUE
    );
}
