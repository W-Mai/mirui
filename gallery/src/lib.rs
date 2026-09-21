//! Gallery shared runner. Backend chosen by feature flag (priority:
//! `web-canvas` on wasm32 > `wgpu` > `sdl-gpu` > `sdl`).
//!
//! ```ignore
//! use gallery::prelude::*;
//!
//! fn main() {
//!     gallery::run("my demo", 480, 320, |setup| {
//!         let root = WidgetBuilder::new(&mut setup.app.world)
//!             .bg_color(Color::rgb(30, 30, 46))
//!             .id();
//!         // optional plugins/systems on the App:
//!         //   setup.app.add_system(my_system::system());
//!         //   setup.app.add_plugin(StdInstantClockPlugin::default());
//!         root
//!     });
//! }
//! ```

pub mod prelude {
    pub use mirui::prelude::*;
}

#[doc(hidden)]
pub mod backend_parity;

pub use mirui;

use mirui::app::{App, RendererFactory};
use mirui::ecs::Entity;
use mirui::surface::Surface;

pub struct SetupGeneric<'a, B: Surface, F: RendererFactory<B>> {
    pub app: &'a mut App<B, F>,
}

pub struct DemoEntry {
    pub slug: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub size: mirui::gallery::DemoSize,
    pub setup: fn(&mut Setup<'_>) -> Entity,
    pub source: &'static str,
}

const FOCUS_START: &str = "//~focus-start";
const FOCUS_END: &str = "//~focus-end";

/// Returns the lines inside `//~focus-start` / `//~focus-end` pairs,
/// dedented to their shallowest common indentation. Sources without
/// any marker fall back to the full text so annotation stays opt-in.
pub fn extract_focus(src: &str) -> String {
    let mut regions: Vec<&str> = Vec::new();
    let mut in_focus = false;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(FOCUS_START) {
            in_focus = true;
            continue;
        }
        if trimmed.starts_with(FOCUS_END) {
            in_focus = false;
            continue;
        }
        if in_focus {
            regions.push(line);
        }
    }

    if regions.is_empty() {
        return src.to_string();
    }

    let min_indent = regions
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut out = String::new();
    for (i, line) in regions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(&line[min_indent..]);
    }
    out
}

#[macro_export]
macro_rules! register_demos {
    ( $( ($slug:literal, $label:literal, $category:literal, $module:ident) ),* $(,)? ) => {
        pub const DEMOS: &[$crate::DemoEntry] = &[
            $(
                $crate::DemoEntry {
                    slug: $slug,
                    label: $label,
                    category: $category,
                    size: $crate::mirui::gallery::demos::$module::DEMO_SIZE,
                    setup: |setup| {
                        let parent = setup.app.spawn_root().id();
                        $crate::mirui::gallery::demos::$module::setup_app(setup.app, parent);
                        parent
                    },
                    source: include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../src/gallery/demos/",
                        stringify!($module),
                        ".rs"
                    )),
                },
            )*
        ];

        pub fn lookup_demo(slug: &str) -> Option<&'static $crate::DemoEntry> {
            DEMOS.iter().find(|d| d.slug == slug)
        }

        pub fn setup_demo<B, F>(
            slug: &str,
            app: &mut $crate::mirui::app::App<B, F>,
        ) -> Option<$crate::mirui::ecs::Entity>
        where
            B: $crate::mirui::surface::Surface,
            F: $crate::mirui::app::RendererFactory<B>,
        {
            match slug {
                $(
                    $slug => {
                        let parent = app.spawn_root().id();
                        $crate::mirui::gallery::demos::$module::setup_app(app, parent);
                        Some(parent)
                    }
                )*
                _ => None,
            }
        }
    };
}

#[cfg(any(feature = "web-canvas", feature = "snapshot"))]
register_demos! {
    ("orbit_console",        "Orbit Console",        "Showcase",    orbit_console),
    ("layout_lab",           "Layout Lab",           "Showcase",    layout_lab),
    ("typography_lab",       "Typography Lab",       "Showcase",    typography_lab),
    ("curve_text",           "Kinetic Type",         "Showcase",    curve_text),
    ("curve_text_compact",   "Curve Text Compact",   "Showcase",    curve_text_compact),
    ("interaction_lab",      "Interaction Lab",      "Showcase",    interaction_lab),
    ("kinetic_console",      "Kinetic Console",      "Showcase",    kinetic_console),

    ("signal_scope",         "Signal Scope",         "Product",     signal_scope),

    ("marble_play",          "Marble Play",          "Play",        marble_play),
    ("lumen_lab",            "Lumen Lab",            "Play",        lumen_lab),
    ("pixel_loom",           "Pixel Loom",           "Play",        pixel_loom),
    ("moss_study",           "Moss Study",           "Play",        moss_study),
    ("pocket_post",          "Pocket Post",          "Play",        pocket_post),
    ("logic_circuit",        "Logic Circuit",        "Play",        logic_circuit),
    ("module_factory",       "Module Factory",       "Play",        module_factory),
    ("orbital_mission",      "Orbital Mission",      "Play",        orbital_mission),
    ("tidal_atlas",          "Tidal Atlas",          "Play",        tidal_atlas),
    ("echo_walker",          "Echo Walker",          "Play",        echo_walker),
    ("twin_beacons",         "Twin Beacons",         "Play",        twin_beacons),
    ("folding_ark",          "Folding Ark",          "Play",        folding_ark),
    ("atlas_restoration",    "Atlas Restoration",    "Play",        atlas_restoration),

    ("niche",                "niche slots (@name)",  "Basics",      niche),
    ("i18n",                 "i18n locale toggle",   "Basics",      i18n),

    ("animation",            "tween + ping pong",    "Animation",   animation),
    ("three_body",           "three body",           "Animation",   three_body),
    ("life",                 "game of life",         "Animation",   life),
    ("life_compact",         "life compact",         "Animation",   life_compact),
    ("particles",            "particles",            "Animation",   particles),
    ("butterfly",            "butterfly",            "Animation",   butterfly),
    ("shapes",               "shapes",               "Animation",   shapes),
    ("subpixel",             "subpixel motion",      "Animation",   subpixel),
    ("spatial_anim",         "spatial anim",         "Animation",   spatial_anim),
    ("transform",            "transform",            "Animation",   transform),
    ("image_flip",           "image flip 3d",        "Animation",   image_flip),
    ("flip_card",            "flip card",            "Animation",   flip_card),
    ("book_flip",            "book flip",            "Animation",   book_flip),

    ("effect_panels",        "effect panels",        "Effects",     effect_panels),
    ("effect_glass",         "effect glass",         "Effects",     effect_glass),
    ("offscreen",            "offscreen render",     "Effects",     offscreen),
    ("offscreen_modal",      "offscreen modal",      "Effects",     offscreen_modal),
    ("custom_view",          "custom view (Diamond)","Effects",     custom_view),
    ("vector_mandala",       "vector mandala",       "Effects",     vector_mandala),
    ("icon",                 "icon set",             "Effects",     icon),
    ("composite",            "blit composite modes", "Effects",     composite),
    ("gradient",             "gradient paint",       "Effects",     gradient),
    ("stroke_styles",        "stroke styles",        "Effects",     stroke_styles),
    ("clip_path",            "clip path",            "Effects",     clip_path),
    ("fill_rules",           "fill rules",           "Effects",     fill_rules),
    ("blur_filter",          "blur filter",          "Effects",     blur_filter),
    ("render_showcase",      "render showcase",      "Effects",     render_showcase),

    ("pinch_rotate",         "pinch + rotate",       "Interaction", pinch_rotate),

    ("state_counter",        "reactive counter",     "State",       state_counter),
    ("state_computed",       "reactive computed",    "State",       state_computed),
    ("state_effect",         "reactive effect",      "State",       state_effect),
    ("state_form",           "reactive form",        "State",       state_form),
    ("state_todo",           "reactive todo",        "State",       state_todo),
    ("state_show",           "reactive if / match",  "State",       state_show),
    ("state_list",           "reactive walk list",   "State",       state_list),
    ("state_keyed",          "keyed walk reorder",   "State",       state_keyed),
    ("persistence_counter",  "persistence counter",  "State",       persistence_counter),

    ("scroll",               "scroll",               "Scroll",      scroll),
    ("nested_scroll",        "nested scroll",        "Scroll",      nested_scroll),
    ("lazy_list",            "lazy list",            "Scroll",      lazy_list),
    ("cover_flow",           "cover flow",           "Scroll",      cover_flow),

    ("slider_value_changed", "slider valueChanged",  "Components",  slider_value_changed),
    ("tabbar",               "tabbar",               "Components",  tabbar),
    ("text_input",           "text input",           "Components",  text_input),
    ("theme_swap",           "theme swap",           "Components",  theme_swap),
    ("widgets",              "widgets",              "Components",  widgets),
    ("widgets_compact",      "widgets compact",      "Components",  widgets_compact),
    ("builder_form",         "builder API (no DSL)", "Components",  builder_form),
}

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
mod backend {
    use super::*;
    use mirui::render::ProjectiveFallback;
    use mirui::render::web_canvas::WebCanvasRendererFactory;
    use mirui::surface::web_canvas::WebCanvasSurface;
    use wasm_bindgen::JsCast;

    const DEMO_PROJECTIVE_RGBA_BYTES: usize = 2 * 1024 * 1024;

    pub type ActiveSurface = WebCanvasSurface;
    pub type ActiveFactory = WebCanvasRendererFactory;

    pub fn configured_factory() -> ActiveFactory {
        WebCanvasRendererFactory::new()
            .with_projective_fallback(ProjectiveFallback::new(vec![0; DEMO_PROJECTIVE_RGBA_BYTES]))
    }

    pub fn build_app(_title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let canvas = web_sys::window()
            .expect("window")
            .document()
            .expect("document")
            .get_element_by_id("mirui")
            .expect("canvas element with id=\"mirui\"")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element is not <canvas>");
        // Demo's logical size drives the canvas CSS box; `index.html`
        // ships a default that any demo other than `dsl` overrides.
        let style = canvas.style();
        let _ = style.set_property("width", &format!("{w}px"));
        let _ = style.set_property("height", &format!("{h}px"));
        let backend = WebCanvasSurface::new(canvas);
        assemble_app(backend, configured_factory())
    }

    pub fn grab_canvas() -> WebCanvasSurface {
        let canvas = web_sys::window()
            .expect("window")
            .document()
            .expect("document")
            .get_element_by_id("mirui")
            .expect("canvas element with id=\"mirui\"")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element is not <canvas>");
        WebCanvasSurface::new(canvas)
    }

    pub fn assemble_app(
        backend: ActiveSurface,
        factory: ActiveFactory,
    ) -> App<ActiveSurface, ActiveFactory> {
        let mut app = App::with_factory(backend, factory);
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub use backend::{assemble_app, configured_factory, grab_canvas};

#[cfg(all(
    feature = "snapshot",
    not(feature = "wgpu"),
    not(feature = "sdl-gpu"),
    not(feature = "sdl"),
    not(feature = "linux-fb"),
    not(feature = "linux-drm"),
    not(all(feature = "web-canvas", target_arch = "wasm32")),
))]
mod backend {
    use super::*;
    use mirui::app::SwRendererFactory;
    use mirui::render::texture::ColorFormat;
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::types::PhysicalRect;

    pub type ActiveSurface = FramebufSurface<fn(&[u8], PhysicalRect)>;
    pub type ActiveFactory = SwRendererFactory;

    pub fn build_app(_title: &str, width: u16, height: u16) -> App<ActiveSurface> {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush);
        let mut app = App::new(backend);
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(
    feature = "wgpu",
    not(all(feature = "web-canvas", target_arch = "wasm32"))
))]
mod backend {
    use super::*;
    use mirui::render::wgpu::WgpuRendererFactory;
    use mirui::surface::wgpu_surface::WgpuSurface;

    const DEMO_TARGET_EDIT_BYTES: usize = 2 * 1024 * 1024;

    pub type ActiveSurface = WgpuSurface;
    pub type ActiveFactory = WgpuRendererFactory;

    pub fn build_app(title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let backend = WgpuSurface::new(title, w, h);
        let factory = WgpuRendererFactory::new().with_target_edit_budget(DEMO_TARGET_EDIT_BYTES);
        let mut app = App::with_factory(backend, factory);
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(
    feature = "sdl-gpu",
    not(feature = "wgpu"),
    not(all(feature = "web-canvas", target_arch = "wasm32")),
))]
mod backend {
    use super::*;
    use mirui::render::ProjectiveFallback;
    use mirui::render::sdl_gpu::SdlGpuFactory;
    use mirui::surface::sdl_gpu::SdlGpuSurface;

    const DEMO_PROJECTIVE_RGBA_BYTES: usize = 2 * 1024 * 1024;

    pub type ActiveSurface = SdlGpuSurface;
    pub type ActiveFactory = SdlGpuFactory;

    pub fn build_app(title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let backend = SdlGpuSurface::new(title, w, h);
        let factory = SdlGpuFactory::new()
            .with_projective_fallback(ProjectiveFallback::new(vec![0; DEMO_PROJECTIVE_RGBA_BYTES]));
        let mut app = App::with_factory(backend, factory);
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(
    feature = "sdl",
    not(feature = "wgpu"),
    not(feature = "sdl-gpu"),
    not(all(feature = "web-canvas", target_arch = "wasm32")),
))]
mod backend {
    use super::*;
    use mirui::app::SwRendererFactory;
    use mirui::surface::sdl::SdlSurface;

    pub type ActiveSurface = SdlSurface;
    pub type ActiveFactory = SwRendererFactory;

    pub fn build_app(title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let backend = SdlSurface::new(title, w, h);
        let mut app = App::new(backend);
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(
    feature = "linux-fb",
    target_os = "linux",
    not(feature = "wgpu"),
    not(feature = "sdl-gpu"),
    not(feature = "sdl"),
    not(feature = "linux-drm"),
    not(all(feature = "web-canvas", target_arch = "wasm32")),
))]
mod backend {
    use super::*;
    use mirui::app::SwRendererFactory;
    use mirui::surface::linux::{self, LinuxFbSurface};

    pub type ActiveSurface = LinuxFbSurface;
    pub type ActiveFactory = SwRendererFactory;

    pub fn build_app(_title: &str, _w: u16, _h: u16) -> App<ActiveSurface, ActiveFactory> {
        // fbdev resolution comes from the kernel; demo `w` / `h` are
        // honoured only on backends that own a window.
        // `MIRUI_OVERSCAN_INSET=<n>` per-side inset in %; HDMI panels eat the edges.
        let inset = std::env::var("MIRUI_OVERSCAN_INSET")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0);
        let backend = linux::init(linux::LinuxConfig {
            overscan_inset_percent: inset,
            ..linux::LinuxConfig::default()
        })
        .expect("open /dev/fb0");
        let mut app = App::with_factory(backend, SwRendererFactory::new());
        app.with_default_widgets().with_default_systems();
        app
    }
}

#[cfg(all(
    feature = "linux-drm",
    target_os = "linux",
    not(feature = "wgpu"),
    not(feature = "sdl-gpu"),
    not(feature = "sdl"),
    not(feature = "linux-fb"),
    not(all(feature = "web-canvas", target_arch = "wasm32")),
))]
mod backend {
    use super::*;
    use mirui::app::SwRendererFactory;
    use mirui::surface::linux::{self, LinuxDrmSurface};

    pub type ActiveSurface = LinuxDrmSurface;
    pub type ActiveFactory = SwRendererFactory;

    pub fn build_app(_title: &str, _w: u16, _h: u16) -> App<ActiveSurface, ActiveFactory> {
        let inset = std::env::var("MIRUI_OVERSCAN_INSET")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0);
        let card_path = std::env::var("MIRUI_DRM_CARD").unwrap_or_else(|_| "/dev/dri/card0".into());
        let connector_filter = std::env::var("MIRUI_DRM_CONNECTOR").ok();
        let buffer_count = std::env::var("MIRUI_DRM_BUFFERS")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(2);
        // MIRUI_DRM_MODE=WxH forces a panel mode the connector reports.
        let mode = std::env::var("MIRUI_DRM_MODE").ok().and_then(|raw| {
            let parsed = raw
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse::<u16>().ok()?, h.parse::<u16>().ok()?)));
            if parsed.is_none() {
                eprintln!(
                    "mirui::gallery: MIRUI_DRM_MODE={raw:?} not WxH; falling back to connector default"
                );
            }
            parsed
        });
        let backend = linux::init_drm(linux::LinuxDrmConfig {
            card_path: &card_path,
            connector_filter: connector_filter.as_deref(),
            overscan_inset_percent: inset,
            buffer_count,
            mode,
            ..linux::LinuxDrmConfig::default()
        })
        .expect("open DRM card");
        let mut app = App::with_factory(backend, SwRendererFactory::new());
        app.with_default_widgets().with_default_systems();
        app
    }
}

/// Active-backend setup passed to gallery demos.
pub type Setup<'a> = SetupGeneric<'a, backend::ActiveSurface, backend::ActiveFactory>;
pub use backend::{ActiveFactory, ActiveSurface};

/// Run a demo on the selected backend. Returns on wasm32 so the
/// browser keeps driving frames.
pub fn run<F>(title: &str, w: u16, h: u16, build: F)
where
    F: FnOnce(&mut Setup<'_>) -> Entity,
{
    let mut app = backend::build_app(title, w, h);

    // Every native backend (SDL / SDL-GPU / wgpu / linux-fb) skips
    // `present`/`flush` on idle frames, which is also where vsync
    // would have waited — without a cap the loop hits 60k+ fps and
    // tears against the host compositor. Web canvas runs ticks from
    // `requestAnimationFrame`, so the browser already paces it.
    // 120 covers ProMotion / 120 Hz panels; override with
    // `MIRUI_FPS_CAP=<n>`. 0 disables the cap for benchmarks.
    #[cfg(not(all(feature = "web-canvas", target_arch = "wasm32")))]
    {
        let cap = std::env::var("MIRUI_FPS_CAP")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(120);
        if cap > 0 {
            app.add_plugin(mirui::app::plugins::FrameRateCapPlugin::new(cap));
        }
    }

    let root = {
        let mut setup = Setup { app: &mut app };
        build(&mut setup)
    };
    app.set_root(root);

    #[cfg(not(all(feature = "web-canvas", target_arch = "wasm32")))]
    app.run();

    #[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
    app.into_runner().start_animation_frame();
}

#[cfg(test)]
mod tests {
    use super::extract_focus;

    #[cfg(any(feature = "web-canvas", feature = "snapshot"))]
    #[test]
    fn demo_catalog_has_unique_slugs_and_valid_bounds() {
        use super::DEMOS;
        use std::collections::HashSet;

        assert_eq!(DEMOS.len(), 71);
        let mut slugs = HashSet::new();
        for demo in DEMOS {
            assert!(slugs.insert(demo.slug), "duplicate slug: {}", demo.slug);
            assert!(!demo.label.is_empty());
            assert!(!demo.category.is_empty());
            assert!(
                demo.size.min_width.is_some()
                    || demo.size.min_height.is_some()
                    || demo.size.max_width.is_some()
                    || demo.size.max_height.is_some()
            );
            if let (Some(min), Some(max)) = (demo.size.min_width, demo.size.max_width) {
                assert!(min <= max, "invalid width bounds for {}", demo.slug);
            }
            if let (Some(min), Some(max)) = (demo.size.min_height, demo.size.max_height) {
                assert!(min <= max, "invalid height bounds for {}", demo.slug);
            }
        }
    }

    #[test]
    fn no_markers_returns_full_source() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        assert_eq!(extract_focus(src), src);
    }

    #[test]
    fn single_region_dedented() {
        let src = "use foo;\n//~focus-start\nfn core() {\n    let x = 1;\n}\n//~focus-end\nfn tail() {}\n";
        assert_eq!(extract_focus(src), "fn core() {\n    let x = 1;\n}");
    }

    #[test]
    fn nested_region_keeps_relative_indent() {
        let src = "//~focus-start\n        ui! {\n            row () {}\n        }\n//~focus-end\n";
        assert_eq!(extract_focus(src), "ui! {\n    row () {}\n}");
    }

    #[test]
    fn multiple_regions_joined() {
        let src = "//~focus-start\n    a();\n//~focus-end\nnoise();\n//~focus-start\n    b();\n//~focus-end\n";
        assert_eq!(extract_focus(src), "a();\nb();");
    }

    #[test]
    fn blank_lines_preserved_without_indent() {
        let src = "//~focus-start\n    a();\n\n    b();\n//~focus-end\n";
        assert_eq!(extract_focus(src), "a();\n\nb();");
    }

    #[test]
    fn mixed_depth_dedents_to_shallowest() {
        let src = "//~focus-start\n    a();\n        b();\n//~focus-end\n";
        assert_eq!(extract_focus(src), "a();\n    b();");
    }

    #[test]
    fn start_without_end_runs_to_eof() {
        let src = "head();\n//~focus-start\n    a();\n    b();\n";
        assert_eq!(extract_focus(src), "a();\nb();");
    }

    #[test]
    fn lone_end_is_ignored() {
        let src = "a();\n//~focus-end\nb();\n";
        assert_eq!(extract_focus(src), src);
    }
}
