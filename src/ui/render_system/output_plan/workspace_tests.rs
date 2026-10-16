use crate::ecs::{Entity, World};
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::widgets::MirrorOf;
use crate::ui::{OffscreenRender, Outputs, WidgetTexture};

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

const BACKGROUND: Color = Color::rgb(0, 0, 0);
const RED: Color = Color::rgb(229, 47, 23);
const BLUE: Color = Color::rgb(23, 47, 229);
const GREEN: Color = Color::rgb(29, 191, 71);
const SMALL_BYTES: usize = 4 * 4 * 4;
const LARGE_BYTES: usize = 8 * 4 * 4;
// Three retained outputs and one plane shared by both sequential mirrors.
const SHARED_BUDGET: usize = 2 * SMALL_BYTES + LARGE_BYTES + LARGE_BYTES;

fn test_app(width: u16, height: u16, budget: usize) -> TestApp {
    let mut app = App::headless(width, height);
    app.with_default_widgets()
        .with_offscreen_pool_budget(budget);
    app
}

fn version(world: &World, entity: Entity) -> u64 {
    world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .state(entity)
        .unwrap()
        .current
        .unwrap()
        .version
}

fn output_pixel(world: &World, entity: Entity, x: i32, y: i32) -> Color {
    world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(entity)
        .unwrap()
        .texture()
        .read_pixels()
        .unwrap()
        .get_pixel(x, y)
}

fn set_color(world: &mut World, entity: Entity, color: Color) {
    world.get_mut::<Style>(entity).unwrap().bg_color = Some(color.into());
}

#[cfg(feature = "std")]
#[crate::compose]
fn inline_mirror() {
    ui! {
        Row (width: 8, height: 4) {
            View (id: "source", width: 4, height: 4, bg_color: RED)
            View (id: "mirror", width: 4, height: 4) [
                MirrorOf::new(id("source")),
            ]
        }
    };
}

#[cfg(feature = "std")]
#[test]
fn changing_source_and_rendering_an_inline_mirror_is_allocation_free_after_warmup() {
    let mut app = test_app(8, 4, 2 * SMALL_BYTES);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, inline_mirror());
    let source = app.world.find_by_id("source").unwrap();
    app.render().unwrap();
    for color in [BLUE, RED, BLUE, RED] {
        set_color(&mut app.world, source, color);
        app.render_dirty().unwrap();
    }

    let mut previous_version = version(&app.world, source);
    let allocations = crate::test_support::tracked_allocations(|| {
        for frame in 0..32 {
            let expected = if frame % 2 == 0 { BLUE } else { RED };
            set_color(&mut app.world, source, expected);
            app.render_dirty().unwrap();
            let current_version = version(&app.world, source);
            assert!(current_version > previous_version);
            previous_version = current_version;
            assert_eq!(
                app.backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(5, 1),
                expected
            );
        }
    });
    assert_eq!(allocations, 0);
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<crate::ui::render_system::LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[crate::compose]
fn differently_sized_mirrors() {
    ui! {
        View (width: 16, height: 8) {
            View (
                id: "small_source",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "independent",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: GREEN
            ) [
                OffscreenRender::new(),
            ]
            View (
                id: "large_source",
                position: Position::Absolute,
                left: 8,
                top: 0,
                width: 8,
                height: 4,
                bg_color: BLUE
            )
            View (
                id: "small_mirror",
                position: Position::Absolute,
                left: 0,
                top: 4,
                width: 4,
                height: 4
            ) [
                MirrorOf::new(id("small_source")),
            ]
            View (
                id: "large_mirror",
                position: Position::Absolute,
                left: 8,
                top: 4,
                width: 8,
                height: 4
            ) [
                MirrorOf::new(id("large_source")),
            ]
        }
    };
}

#[test]
fn differently_sized_mirrors_share_one_maximum_plane_at_the_exact_budget() {
    let mut app = test_app(16, 8, SHARED_BUDGET);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, differently_sized_mirrors());
    app.render().unwrap();

    let small = app.world.find_by_id("small_source").unwrap();
    let large = app.world.find_by_id("large_source").unwrap();
    let independent = app.world.find_by_id("independent").unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    for (entity, size) in [(small, (4, 4)), (large, (8, 4)), (independent, (4, 4))] {
        let output = outputs.get(entity).unwrap();
        assert_eq!((output.texture().width, output.texture().height), size);
    }
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 5),
        RED
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(9, 5),
        BLUE
    );
    assert_eq!(output_pixel(&app.world, independent, 1, 1), GREEN);

    for (small_color, large_color) in [(BLUE, RED), (GREEN, BLUE), (RED, GREEN)] {
        set_color(&mut app.world, small, small_color);
        set_color(&mut app.world, large, large_color);
        app.render_dirty().unwrap();
        assert_eq!(
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .get_pixel(1, 5),
            small_color
        );
        assert_eq!(
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .get_pixel(9, 5),
            large_color
        );
        assert_eq!(output_pixel(&app.world, independent, 1, 1), GREEN);
    }
}

#[test]
fn cold_workspace_budget_rejection_skips_effects_but_preserves_independent_outputs() {
    let mut app = test_app(16, 8, SHARED_BUDGET - 1);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, differently_sized_mirrors());
    app.render().unwrap();

    for (name, expected) in [
        ("small_source", RED),
        ("large_source", BLUE),
        ("independent", GREEN),
    ] {
        let entity = app.world.find_by_id(name).unwrap();
        assert_eq!(output_pixel(&app.world, entity, 1, 1), expected);
    }
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 1), RED);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(9, 1), BLUE);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(5, 1), GREEN);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 5), BACKGROUND);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(9, 5), BACKGROUND);
}

#[crate::compose]
fn removable_mirror() {
    ui! {
        Row (width: 12, height: 4) {
            View (id: "source", width: 4, height: 4, bg_color: RED) [
                OffscreenRender::new(),
            ]
            View (id: "mirror", width: 4, height: 4) [
                MirrorOf::new(id("source")),
            ]
        }
    };
}

#[test]
fn removing_a_mirror_releases_workspace_before_its_explicit_source_grows() {
    let mut app = test_app(12, 4, 2 * SMALL_BYTES);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, removable_mirror());
    let source = app.world.find_by_id("source").unwrap();
    let mirror = app.world.find_by_id("mirror").unwrap();
    app.render().unwrap();
    assert_eq!(output_pixel(&app.world, source, 3, 1), RED);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(5, 1),
        RED
    );
    let first_version = version(&app.world, source);

    assert!(app.world.remove::<MirrorOf>(mirror).is_some());
    let style = app.world.get_mut::<Style>(source).unwrap();
    style.layout.width = Dimension::px(8);
    style.bg_color = Some(BLUE.into());
    app.render_dirty().unwrap();

    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(!outputs.is_requested(source));
    assert!(app.world.get::<OffscreenRender>(source).is_some());
    let current = outputs
        .get(source)
        .expect("released scratch admits the larger source");
    assert_eq!((current.texture().width, current.texture().height), (8, 4));
    assert_eq!(
        current.texture().read_pixels().unwrap().get_pixel(7, 1),
        BLUE
    );
    assert_eq!(version(&app.world, source), first_version + 1);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(7, 1),
        BLUE
    );
}

#[crate::compose]
fn mirror_inside_capture() {
    ui! {
        Row (width: 8, height: 4) {
            Column (id: "striped_source", width: 4, height: 4) {
                View (id: "source_top", width: 4, height: 2, bg_color: RED)
                View (id: "source_bottom", width: 4, height: 2, bg_color: BLUE)
            }
            View (id: "capture", width: 4, height: 4) [
                OffscreenRender::new(),
            ] {
                View (id: "nested_mirror", width: 4, height: 4) [
                    MirrorOf::new(id("striped_source")),
                ]
            }
        }
    };
}

#[test]
fn removing_all_output_demand_releases_the_retained_workspace() {
    let mut app = test_app(12, 4, 2 * SMALL_BYTES);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, removable_mirror());
    let source = app.world.find_by_id("source").unwrap();
    let mirror = app.world.find_by_id("mirror").unwrap();
    app.render().unwrap();
    app.world.remove::<MirrorOf>(mirror);
    app.world.remove::<OffscreenRender>(source);
    app.render_dirty().unwrap();

    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(outputs.get(source).is_none());
    let plan = outputs.take_texture_plan().unwrap();
    assert_eq!(plan.workspace.capacity(), 0);
    outputs.put_texture_plan(plan);
}

#[test]
fn inline_mirror_inside_a_capture_uses_the_prepared_workspace() {
    let mut app = test_app(8, 4, 3 * SMALL_BYTES);
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    app.compose(root, mirror_inside_capture());
    let source = app.world.find_by_id("striped_source").unwrap();
    let top = app.world.find_by_id("source_top").unwrap();
    let bottom = app.world.find_by_id("source_bottom").unwrap();
    let capture = app.world.find_by_id("capture").unwrap();
    app.render().unwrap();
    assert_eq!(output_pixel(&app.world, capture, 1, 0), BLUE);
    assert_eq!(output_pixel(&app.world, capture, 1, 3), RED);

    let mut previous_version = version(&app.world, capture);
    for (top_color, bottom_color) in [(BLUE, RED), (GREEN, BLUE), (RED, GREEN)] {
        set_color(&mut app.world, top, top_color);
        set_color(&mut app.world, bottom, bottom_color);
        app.render_dirty().unwrap();
        assert!(version(&app.world, capture) > previous_version);
        assert!(version(&app.world, source) < version(&app.world, capture));
        previous_version = version(&app.world, capture);
        assert_eq!(output_pixel(&app.world, capture, 1, 0), bottom_color);
        assert_eq!(output_pixel(&app.world, capture, 1, 3), top_color);
        assert_eq!(
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .get_pixel(5, 0),
            bottom_color
        );
        assert_eq!(
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .get_pixel(5, 3),
            top_color
        );
    }

    #[cfg(feature = "std")]
    {
        let allocations = crate::test_support::tracked_allocations(|| {
            for frame in 0..32 {
                let color = if frame % 2 == 0 { BLUE } else { RED };
                set_color(&mut app.world, top, color);
                app.render_dirty().unwrap();
                assert!(version(&app.world, capture) > previous_version);
                previous_version = version(&app.world, capture);
                assert_eq!(output_pixel(&app.world, capture, 1, 3), color);
                assert_eq!(
                    app.backend
                        .framebuffer()
                        .read_pixels()
                        .unwrap()
                        .get_pixel(5, 3),
                    color
                );
            }
        });
        assert_eq!(allocations, 0);
    }

    let mut denied = test_app(8, 4, 3 * SMALL_BYTES - 1);
    let root = denied.spawn_root().bg_color(BACKGROUND).id();
    denied.compose(root, mirror_inside_capture());
    denied.render().unwrap();
    let source = denied.world.find_by_id("striped_source").unwrap();
    let capture = denied.world.find_by_id("capture").unwrap();
    let outputs = denied.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(outputs.get(source).is_some());
    assert!(outputs.get(capture).is_none());
    assert_eq!(
        denied
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(5, 1),
        BACKGROUND
    );
}
