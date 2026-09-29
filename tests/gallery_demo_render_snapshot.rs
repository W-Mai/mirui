#![cfg(feature = "gallery")]

extern crate alloc;

use mirui::ecs::World;
use mirui::prelude::*;
use mirui::render::SwRendererFactory;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::render_system;

type SnapshotBackend = FramebufSurface<fn(&[u8], mirui::types::PhysicalRect)>;
type SnapshotApp = App<SnapshotBackend, SwRendererFactory>;

fn snapshot_app(width: u16, height: u16) -> SnapshotApp {
    let flush: fn(&[u8], mirui::types::PhysicalRect) = |_, _| {};
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(mirui::app::plugins::StdInstantClockPlugin);
    app.add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());
    app
}

fn snapshot_parent(app: &mut SnapshotApp) -> mirui::ecs::Entity {
    WidgetBuilder::new(&mut app.world)
        .layout(mirui::ui::layout::LayoutStyle {
            direction: mirui::ui::layout::FlexDirection::Column,
            grow: Fixed::ONE,
            ..Default::default()
        })
        .id()
}

fn finish_snapshot(
    app: &mut SnapshotApp,
    parent: mirui::ecs::Entity,
    width: u16,
    height: u16,
) -> alloc::collections::BTreeSet<(u8, u8, u8)> {
    app.systems.run_all(&mut app.world);

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, parent, &viewport);
    let tex = app.backend.framebuffer();
    let mut renderer = SwRenderer::new(tex);
    renderer.viewport = viewport;
    render_system::render(&app.world, parent, &viewport, &mut renderer).unwrap();

    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;
    let mut colours = alloc::collections::BTreeSet::new();
    for y in 0..(height as usize) {
        for x in 0..(width as usize) {
            let i = y * stride + x * 4;
            let r = pixels[i] & 0xF0;
            let g = pixels[i + 1] & 0xF0;
            let b = pixels[i + 2] & 0xF0;
            colours.insert((r, g, b));
        }
    }
    colours
}

/// Render the demo, return distinct quantized RGB colours encountered.
/// A collapsed layout shows only the root bg (1 colour); real widget
/// content emits multiple distinct colours.
fn render_demo<F: FnOnce(&mut World, mirui::ecs::Entity)>(
    width: u16,
    height: u16,
    build: F,
) -> alloc::collections::BTreeSet<(u8, u8, u8)> {
    let mut app = snapshot_app(width, height);
    let parent = snapshot_parent(&mut app);
    build(&mut app.world, parent);
    finish_snapshot(&mut app, parent, width, height)
}

fn render_app_demo<F: FnOnce(&mut SnapshotApp, mirui::ecs::Entity)>(
    width: u16,
    height: u16,
    build: F,
) -> alloc::collections::BTreeSet<(u8, u8, u8)> {
    let mut app = snapshot_app(width, height);
    let parent = snapshot_parent(&mut app);
    build(&mut app, parent);
    finish_snapshot(&mut app, parent, width, height)
}

/// Threshold: collapsed layout shows just the root bg (1 colour).
/// Even minimal demos (single widget on bg) emit ≥ 2; the typical
/// demo emits 4-25. 3 separates working from broken.
const MIN_COLOURS: usize = 3;

fn assert_renders(name: &str, colours: alloc::collections::BTreeSet<(u8, u8, u8)>) {
    println!("  {name}: {} distinct colours", colours.len());
    assert!(
        colours.len() >= MIN_COLOURS,
        "{name}: only {} colour(s) — layout collapsed",
        colours.len(),
    );
}

macro_rules! basic_demo_scoped {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                let mut cx = mirui::ui::UiScope::new(world, parent);
                mirui::gallery::demos::$name::build_widgets(&mut cx);
            });
            assert_renders(stringify!($name), cs);
        }
    };
}

macro_rules! viewport_demo_scoped {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                let mut cx = mirui::ui::UiScope::new(world, parent);
                mirui::gallery::demos::$name::build_widgets(&mut cx, $w, $h);
            });
            assert_renders(stringify!($name), cs);
        }
    };
}

// These demos depend on multi-frame state evolution that a single
// `systems.run_all + render` snapshot can't reproduce: WidgetTransform3D
// composition, Custom View animation seeds, or LazyList pool warm-up.
// They are validated end-to-end via `cargo run -p gallery --example
// <name>_demo` and ESP feature builds; this snapshot harness only
// gates the in-place layout fix, which all 40 other demos exercise.
macro_rules! viewport_demo_ignored_scoped {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        #[ignore = "needs multi-frame example loop, see module note above"]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                let mut cx = mirui::ui::UiScope::new(world, parent);
                mirui::gallery::demos::$name::build_widgets(&mut cx, $w, $h);
            });
            assert_renders(stringify!($name), cs);
        }
    };
}

macro_rules! viewport_demo_ignored_noargs_scoped {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        #[ignore = "needs multi-frame example loop, see module note above"]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                let mut cx = mirui::ui::UiScope::new(world, parent);
                mirui::gallery::demos::$name::build_widgets(&mut cx);
            });
            assert_renders(stringify!($name), cs);
        }
    };
}

basic_demo_scoped!(animation, 320, 180);
basic_demo_scoped!(book_flip, 640, 360);
basic_demo_scoped!(image_flip, 480, 320);
#[test]
fn interaction_lab() {
    let cs = render_app_demo(1024, 720, |app, parent| {
        mirui::gallery::demos::interaction_lab::setup_app(app, parent);
    });
    assert_renders("interaction_lab", cs);
}
basic_demo_scoped!(layout_lab, 1024, 720);
// Skip: LazyList pool warm-up needs multi-frame loop.
// basic_demo_scoped!(lazy_list, 320, 320);
basic_demo_scoped!(nested_scroll, 480, 400);
basic_demo_scoped!(offscreen, 360, 360);
basic_demo_scoped!(offscreen_modal, 360, 360);
#[test]
fn pinch_rotate() {
    let cs = render_app_demo(480, 360, |app, parent| {
        mirui::gallery::demos::pinch_rotate::setup_app(app, parent);
    });
    assert_renders("pinch_rotate", cs);
}
basic_demo_scoped!(scroll, 480, 320);
basic_demo_scoped!(slider_value_changed, 720, 320);
basic_demo_scoped!(spatial_anim, 400, 300);
basic_demo_scoped!(tabbar, 480, 320);
basic_demo_scoped!(text_input, 480, 200);
basic_demo_scoped!(theme_swap, 480, 320);
basic_demo_scoped!(transform, 480, 320);

#[test]
fn themed_demos_render_in_light_palette() {
    let layout = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::layout_lab::build_widgets(&mut cx);
    });
    assert_renders("layout_lab_light", layout);

    let interaction = render_app_demo(320, 568, |app, parent| {
        mirui::gallery::demos::interaction_lab::setup_app(app, parent);
        app.set_theme(mirui::gallery::showcase_theme::LIGHT_ID)
            .unwrap();
    });
    assert_renders("interaction_lab_light", interaction);

    let scroll = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::scroll::build_widgets(&mut cx);
    });
    assert_renders("scroll_light", scroll);

    let tabs = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::tabbar::build_widgets(&mut cx);
    });
    assert_renders("tabbar_light", tabs);

    let book = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::book_flip::build_widgets(&mut cx);
    });
    assert_renders("book_flip_light", book);

    let card = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::flip_card::build_widgets(&mut cx);
        drop(cx);
        world.insert_resource(mirui::ecs::DeltaTimeMs(16));
        mirui::gallery::demos::flip_card::flip_system(world);
    });
    assert_renders("flip_card_light", card);

    let image = render_demo(320, 568, |world, parent| {
        world.insert_resource(mirui::ui::Theme::light());
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::image_flip::build_widgets(&mut cx);
    });
    assert_renders("image_flip_light", image);
}

basic_demo_scoped!(effect_panels, 480, 360);
basic_demo_scoped!(effect_glass, 128, 128);
basic_demo_scoped!(particles, 480, 320);
basic_demo_scoped!(subpixel, 480, 320);
viewport_demo_scoped!(widgets, 512, 512);
basic_demo_scoped!(widgets_compact, 128, 128);

#[test]
fn life_compact() {
    let cs = render_demo(128, 128, |world, parent| {
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::life_compact::build_widgets(&mut cx, 128, 128);
    });
    assert_renders("life_compact", cs);
}

viewport_demo_ignored_noargs_scoped!(butterfly, 480, 480);
viewport_demo_ignored_scoped!(cover_flow, 640, 360);
viewport_demo_ignored_noargs_scoped!(flip_card, 480, 320);
viewport_demo_ignored_noargs_scoped!(shapes, 480, 480);

fn render_typography_lab(light: bool) -> alloc::collections::BTreeSet<(u8, u8, u8)> {
    let (width, height) = mirui::gallery::demos::typography_lab::VIEWPORT;
    render_app_demo(width, height, |app, parent| {
        mirui::gallery::demos::typography_lab::setup_app(app, parent);
        if light {
            app.set_theme(mirui::gallery::showcase_theme::LIGHT_ID)
                .unwrap();
        }
    })
}

#[test]
fn typography_lab_renders() {
    let colours = render_typography_lab(false);
    assert_renders("typography_lab", colours);
}

#[test]
fn typography_lab_renders_in_light_palette() {
    let dark = render_typography_lab(false);
    let light = render_typography_lab(true);
    assert_renders("typography_lab_light", light.clone());
    assert_ne!(dark, light);
}

#[test]
fn three_body_renders() {
    let cs = render_demo(480, 320, |world, parent| {
        let mut cx = mirui::ui::UiScope::new(world, parent);
        mirui::gallery::demos::three_body::build_widgets(&mut cx, 480, 320, 3, Fixed::from_int(30));
    });
    assert_renders("three_body", cs);
}

#[test]
fn custom_view_renders() {
    use mirui::ui::view::ViewRegistry;
    let backend = FramebufSurface::with_format(480, 200, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    if let Some(reg) = app.world.resource_mut::<ViewRegistry>() {
        reg.insert(mirui::gallery::demos::custom_view::diamond_view());
    }

    let parent = WidgetBuilder::new(&mut app.world).id();
    let mut cx = mirui::ui::UiScope::new(&mut app.world, parent);
    mirui::gallery::demos::custom_view::build_widgets(&mut cx);
    drop(cx);

    let viewport = Viewport::new(480, 200, Fixed::ONE);
    render_system::update_layout(&mut app.world, parent, &viewport);
    let tex = app.backend.framebuffer();
    let mut renderer = SwRenderer::new(tex);
    renderer.viewport = viewport;
    render_system::render(&app.world, parent, &viewport, &mut renderer).unwrap();

    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;
    let mut colours = alloc::collections::BTreeSet::new();
    for y in 0..200usize {
        for x in 0..480usize {
            let i = y * stride + x * 4;
            colours.insert((pixels[i] & 0xF0, pixels[i + 1] & 0xF0, pixels[i + 2] & 0xF0));
        }
    }
    assert_renders("custom_view", colours);
}
