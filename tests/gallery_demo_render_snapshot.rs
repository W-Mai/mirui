#![cfg(feature = "gallery")]

extern crate alloc;

use mirui::ecs::World;
use mirui::prelude::*;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::render_system;

/// Render the demo, return distinct quantized RGB colours encountered.
/// A collapsed layout shows only the root bg (1 colour); real widget
/// content emits multiple distinct colours.
fn render_demo<F: FnOnce(&mut World, mirui::ecs::Entity)>(
    width: u16,
    height: u16,
    build: F,
) -> alloc::collections::BTreeSet<(u8, u8, u8)> {
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(mirui::app::plugins::StdInstantClockPlugin);
    app.add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());
    let parent = WidgetBuilder::new(&mut app.world)
        .layout(mirui::ui::layout::LayoutStyle {
            direction: mirui::ui::layout::FlexDirection::Column,
            grow: Fixed::ONE,
            ..Default::default()
        })
        .id();
    build(&mut app.world, parent);

    app.systems.run_all(&mut app.world);

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, parent, &viewport);
    let tex = app.backend.framebuffer();
    let mut renderer = SwRenderer::new(tex);
    renderer.viewport = viewport;
    render_system::render(&app.world, parent, &viewport, &mut renderer);

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

macro_rules! basic_demo {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                mirui::gallery::demos::$name::build_widgets(world, parent);
            });
            assert_renders(stringify!($name), cs);
        }
    };
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

macro_rules! viewport_demo_ignored_noargs {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        #[ignore = "needs multi-frame example loop, see module note above"]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                mirui::gallery::demos::$name::build_widgets(world, parent);
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

basic_demo_scoped!(absolute, 480, 320);
basic_demo_scoped!(animation, 320, 180);
basic_demo_scoped!(app_demo, 480, 320);
basic_demo_scoped!(book_flip, 640, 360);
basic_demo_scoped!(click, 480, 320);
basic_demo_scoped!(components, 480, 320);
basic_demo_scoped!(disabled, 480, 320);
basic_demo_scoped!(dsl, 480, 320);
basic_demo_scoped!(enchants, 480, 320);
basic_demo_scoped!(gesture, 320, 240);
basic_demo_scoped!(hello, 480, 320);
basic_demo_scoped!(hover_tour, 720, 360);
basic_demo_scoped!(image, 480, 320);
basic_demo!(image_flip, 480, 320);
basic_demo_scoped!(input_feedback, 640, 360);
basic_demo_scoped!(interactive_states, 720, 420);
// Skip: LazyList pool warm-up needs multi-frame loop.
// basic_demo_scoped!(lazy_list, 320, 320);
basic_demo_scoped!(nested_scroll, 480, 400);
basic_demo_scoped!(offscreen, 360, 360);
basic_demo_scoped!(offscreen_modal, 360, 360);
basic_demo_scoped!(on_handlers, 640, 320);
basic_demo_scoped!(pinch_rotate, 480, 360);
basic_demo_scoped!(rounded, 480, 320);
basic_demo_scoped!(scroll, 480, 320);
basic_demo_scoped!(slider_switch, 320, 200);
basic_demo_scoped!(slider_value_changed, 720, 320);
basic_demo_scoped!(spatial_anim, 400, 300);
basic_demo_scoped!(tabbar, 480, 320);
basic_demo_scoped!(tabbar_selection, 640, 320);
basic_demo_scoped!(text, 480, 320);
basic_demo_scoped!(text_input, 480, 200);
basic_demo_scoped!(theme_swap, 480, 320);
basic_demo_scoped!(toggle, 640, 320);
basic_demo!(transform, 480, 320);
basic_demo_scoped!(walk, 480, 320);

basic_demo_scoped!(effect_panels, 480, 360);
basic_demo_scoped!(effect_glass, 128, 128);
basic_demo!(particles, 480, 320);
basic_demo!(subpixel, 480, 320);
viewport_demo_scoped!(widgets, 512, 512);

viewport_demo_ignored_noargs_scoped!(butterfly, 480, 480);
viewport_demo_ignored_scoped!(cover_flow, 640, 360);
viewport_demo_ignored_noargs!(flip_card, 480, 320);
viewport_demo_ignored_noargs_scoped!(shapes, 480, 480);

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
    render_system::render(&app.world, parent, &viewport, &mut renderer);

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
