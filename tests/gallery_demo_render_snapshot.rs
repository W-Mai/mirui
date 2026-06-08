#![cfg(feature = "gallery")]

extern crate alloc;

use mirui::draw::sw::SwRenderer;
use mirui::draw::texture::ColorFormat;
use mirui::ecs::World;
use mirui::prelude::*;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::render_system;

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
    app.add_plugin(mirui::plugins::StdInstantClockPlugin);
    let parent = WidgetBuilder::new(&mut app.world).id();
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

macro_rules! viewport_demo {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                mirui::gallery::demos::$name::build_widgets(world, parent, $w, $h);
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
macro_rules! viewport_demo_ignored {
    ($name:ident, $w:expr, $h:expr) => {
        #[test]
        #[ignore = "needs multi-frame example loop, see module note above"]
        fn $name() {
            let cs = render_demo($w, $h, |world, parent| {
                mirui::gallery::demos::$name::build_widgets(world, parent, $w, $h);
            });
            assert_renders(stringify!($name), cs);
        }
    };
}

basic_demo!(absolute, 480, 320);
basic_demo!(animation, 320, 180);
basic_demo!(app_demo, 480, 320);
basic_demo!(book_flip, 640, 360);
basic_demo!(click, 480, 320);
basic_demo!(components, 480, 320);
basic_demo!(disabled, 480, 320);
basic_demo!(dsl, 480, 320);
basic_demo!(enchants, 480, 320);
basic_demo!(gesture, 320, 240);
basic_demo!(hello, 480, 320);
basic_demo!(hover_tour, 720, 360);
basic_demo!(image, 480, 320);
basic_demo!(image_flip, 480, 320);
basic_demo!(input_feedback, 640, 360);
basic_demo!(interactive_states, 720, 420);
// Skip: LazyList pool warm-up needs multi-frame loop.
// basic_demo!(lazy_list, 320, 320);
basic_demo!(nested_scroll, 480, 400);
basic_demo!(offscreen, 360, 360);
basic_demo!(offscreen_modal, 360, 360);
basic_demo!(on_handlers, 640, 320);
basic_demo!(pinch_rotate, 480, 360);
basic_demo!(rounded, 480, 320);
basic_demo!(scroll, 480, 320);
basic_demo!(slider_switch, 320, 200);
basic_demo!(slider_value_changed, 720, 320);
basic_demo!(spatial_anim, 400, 300);
basic_demo!(tabbar, 480, 320);
basic_demo!(tabbar_selection, 640, 320);
basic_demo!(text, 480, 320);
basic_demo!(text_input, 480, 200);
basic_demo!(theme_swap, 480, 320);
basic_demo!(toggle, 640, 320);
basic_demo!(transform, 480, 320);
basic_demo!(walk, 480, 320);

viewport_demo!(effect, 480, 360);
viewport_demo!(particles, 480, 320);
viewport_demo!(subpixel, 480, 320);
viewport_demo!(widgets, 512, 512);

viewport_demo_ignored!(butterfly, 480, 480);
viewport_demo_ignored!(cover_flow, 640, 360);
viewport_demo_ignored!(flip_card, 480, 320);
viewport_demo_ignored!(shapes, 480, 480);

#[test]
fn three_body_renders() {
    let cs = render_demo(480, 320, |world, parent| {
        mirui::gallery::demos::three_body::build_widgets(
            world,
            parent,
            480,
            320,
            3,
            Fixed::from_int(30),
        );
    });
    assert_renders("three_body", cs);
}

#[test]
fn custom_view_renders() {
    use mirui::widget::view::ViewRegistry;
    let backend = FramebufSurface::with_format(480, 200, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    if let Some(reg) = app.world.resource_mut::<ViewRegistry>() {
        reg.insert(mirui::gallery::demos::custom_view::diamond_view());
    }

    let parent = WidgetBuilder::new(&mut app.world).id();
    mirui::gallery::demos::custom_view::build_widgets(&mut app.world, parent);

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
