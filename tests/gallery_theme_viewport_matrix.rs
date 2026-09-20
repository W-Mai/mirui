#![cfg(feature = "gallery")]

extern crate alloc;

use alloc::collections::BTreeSet;
use mirui::app::App;
use mirui::core::reactive::Signal;
use mirui::surface::FramebufferAccess;
use mirui::ui::Theme;

#[derive(Clone, Copy)]
struct FrameSignature {
    hash: u64,
    colours: usize,
}

fn frame_signature<B: FramebufferAccess>(
    backend: &mut B,
    width: u16,
    height: u16,
) -> FrameSignature {
    let texture = backend.framebuffer();
    let pixels = texture.buf.as_slice();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut colour_bins = [false; 4096];
    for y in 0..usize::from(height) {
        for x in 0..usize::from(width) {
            let index = y * texture.stride + x * 4;
            let pixel = &pixels[index..index + 4];
            for byte in pixel {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            let colour_index = (usize::from(pixel[0] >> 4) << 8)
                | (usize::from(pixel[1] >> 4) << 4)
                | usize::from(pixel[2] >> 4);
            colour_bins[colour_index] = true;
        }
    }
    FrameSignature {
        hash,
        colours: colour_bins.into_iter().filter(|present| *present).count(),
    }
}

macro_rules! render_demo {
    ($module:ident, $width:expr, $height:expr, $theme:expr) => {{
        let width = $width;
        let height = $height;
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        mirui::gallery::demos::$module::setup_app(&mut app, root);
        app.set_root(root);
        app.set_theme($theme).unwrap();
        app.systems.run_all(&mut app.world);
        app.render().unwrap();
        frame_signature(&mut app.backend, width, height)
    }};
}

macro_rules! render_persistence_counter {
    ($width:expr, $height:expr, $theme:expr) => {{
        let width = $width;
        let height = $height;
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let count = Signal::new(0_i32);
        app.compose(root, |cx| {
            mirui::gallery::demos::persistence_counter::build_widgets(cx, count)
        });
        app.set_root(root);
        app.set_theme($theme).unwrap();
        app.systems.run_all(&mut app.world);
        app.render().unwrap();
        frame_signature(&mut app.backend, width, height)
    }};
}

fn assert_theme_pair(
    module: &str,
    width: u16,
    height: u16,
    light: FrameSignature,
    dark: FrameSignature,
) {
    assert!(
        light.colours >= 3,
        "{module} light {width}x{height} collapsed to {} colours",
        light.colours,
    );
    assert!(
        dark.colours >= 3,
        "{module} dark {width}x{height} collapsed to {} colours",
        dark.colours,
    );
    assert_ne!(
        light.hash, dark.hash,
        "{module} produced identical light and dark frames at {width}x{height}",
    );
}

fn registered_modules() -> BTreeSet<&'static str> {
    include_str!("../gallery/web/src/lib.rs")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("(\"") {
                return None;
            }
            line.split(',').nth(3).map(str::trim)
        })
        .collect()
}

#[test]
fn registered_demos_render_in_both_themes_across_supported_viewports() {
    let mut covered = BTreeSet::new();

    macro_rules! responsive {
        ($module:ident) => {{
            covered.insert(stringify!($module));
            for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
                let light = render_demo!($module, width, height, Theme::light());
                let dark = render_demo!($module, width, height, Theme::dark());
                assert_theme_pair(stringify!($module), width, height, light, dark);
            }
        }};
    }

    macro_rules! compact {
        ($module:ident) => {{
            covered.insert(stringify!($module));
            let light = render_demo!($module, 128, 128, Theme::light());
            let dark = render_demo!($module, 128, 128, Theme::dark());
            assert_theme_pair(stringify!($module), 128, 128, light, dark);
        }};
    }

    macro_rules! fixed_palette {
        ($module:ident) => {{
            covered.insert(stringify!($module));
            for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
                for theme in [Theme::light(), Theme::dark()] {
                    let frame = render_demo!($module, width, height, theme);
                    assert!(
                        frame.colours >= 3,
                        "{} {width}x{height} collapsed to {} colours",
                        stringify!($module),
                        frame.colours,
                    );
                }
            }
        }};
    }

    macro_rules! persistence_counter {
        () => {{
            covered.insert("persistence_counter");
            for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
                let light = render_persistence_counter!(width, height, Theme::light());
                let dark = render_persistence_counter!(width, height, Theme::dark());
                assert_theme_pair("persistence_counter", width, height, light, dark);
            }
        }};
    }

    macro_rules! layout_only {
        ($module:ident) => {{
            covered.insert(stringify!($module));
            for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
                for theme in [Theme::light(), Theme::dark()] {
                    let mut app = App::headless(width, height);
                    app.with_default_widgets().with_default_systems();
                    let root = app.spawn_root().id();
                    mirui::gallery::demos::$module::setup_app(&mut app, root);
                    app.set_root(root);
                    app.set_theme(theme).unwrap();
                    app.systems.run_all(&mut app.world);
                    mirui::ui::render_system::update_layout(
                        &mut app.world,
                        root,
                        &mirui::types::Viewport::new(width, height, mirui::types::Fixed::ONE),
                    );
                    let visible_rects = app
                        .world
                        .query::<mirui::ui::ComputedRect>()
                        .iter()
                        .filter(|(_, rect)| rect.0.w.is_positive() && rect.0.h.is_positive())
                        .count();
                    assert!(
                        visible_rects >= 2,
                        "{} produced only {visible_rects} visible layout rectangles at {width}x{height}",
                        stringify!($module),
                    );
                }
            }
        }};
    }

    responsive!(orbit_console);
    responsive!(layout_lab);
    responsive!(typography_lab);
    responsive!(curve_text);
    compact!(curve_text_compact);
    responsive!(interaction_lab);
    compact!(kinetic_console);
    fixed_palette!(signal_scope);
    fixed_palette!(marble_play);
    responsive!(niche);
    responsive!(i18n);
    responsive!(animation);
    responsive!(three_body);
    responsive!(life);
    compact!(life_compact);
    responsive!(particles);
    responsive!(butterfly);
    responsive!(shapes);
    responsive!(subpixel);
    responsive!(spatial_anim);
    responsive!(transform);
    responsive!(image_flip);
    responsive!(flip_card);
    responsive!(book_flip);
    responsive!(effect_panels);
    compact!(effect_glass);
    responsive!(offscreen);
    responsive!(offscreen_modal);
    responsive!(custom_view);
    layout_only!(vector_mandala);
    responsive!(icon);
    responsive!(composite);
    responsive!(gradient);
    responsive!(stroke_styles);
    layout_only!(clip_path);
    responsive!(fill_rules);
    layout_only!(blur_filter);
    layout_only!(render_showcase);
    responsive!(pinch_rotate);
    responsive!(state_counter);
    responsive!(state_computed);
    responsive!(state_effect);
    responsive!(state_form);
    responsive!(state_todo);
    responsive!(state_show);
    responsive!(state_list);
    responsive!(state_keyed);
    persistence_counter!();
    responsive!(scroll);
    responsive!(nested_scroll);
    responsive!(lazy_list);
    responsive!(cover_flow);
    responsive!(slider_value_changed);
    responsive!(tabbar);
    responsive!(text_input);
    responsive!(theme_swap);
    responsive!(widgets);
    compact!(widgets_compact);
    responsive!(builder_form);

    assert_eq!(covered, registered_modules());
}
