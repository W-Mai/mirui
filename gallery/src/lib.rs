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
    pub width: u16,
    pub height: u16,
    pub setup: fn(&mut Setup<'_>) -> Entity,
}

#[macro_export]
macro_rules! register_demos {
    ( $( ($slug:literal, $label:literal, $category:literal, $module:ident, $w:literal, $h:literal) ),* $(,)? ) => {
        pub const DEMOS: &[$crate::DemoEntry] = &[
            $(
                $crate::DemoEntry {
                    slug: $slug,
                    label: $label,
                    category: $category,
                    width: $w,
                    height: $h,
                    setup: |setup| {
                        let parent = setup.app.spawn_root().id();
                        $crate::mirui::gallery::demos::$module::setup_app(setup.app, parent);
                        parent
                    },
                },
            )*
        ];

        pub fn lookup_demo(slug: &str) -> Option<&'static $crate::DemoEntry> {
            DEMOS.iter().find(|d| d.slug == slug)
        }
    };
}

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
mod backend {
    use super::*;
    use mirui::draw::web_canvas::WebCanvasRendererFactory;
    use mirui::surface::web_canvas::WebCanvasSurface;
    use wasm_bindgen::JsCast;

    pub type ActiveSurface = WebCanvasSurface;
    pub type ActiveFactory = WebCanvasRendererFactory;

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
        let factory = WebCanvasRendererFactory::new();
        let mut app = App::with_factory(backend, factory);
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
    use mirui::draw::wgpu_render::WgpuRendererFactory;
    use mirui::surface::wgpu_surface::WgpuSurface;

    pub type ActiveSurface = WgpuSurface;
    pub type ActiveFactory = WgpuRendererFactory;

    pub fn build_app(title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let backend = WgpuSurface::new(title, w, h);
        let factory = WgpuRendererFactory::new();
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
    use mirui::draw::sdl_gpu::SdlGpuFactory;
    use mirui::surface::sdl_gpu::SdlGpuSurface;

    pub type ActiveSurface = SdlGpuSurface;
    pub type ActiveFactory = SdlGpuFactory;

    pub fn build_app(title: &str, w: u16, h: u16) -> App<ActiveSurface, ActiveFactory> {
        let backend = SdlGpuSurface::new(title, w, h);
        let factory = SdlGpuFactory;
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
        let mut app = App::with_factory(backend, SwRendererFactory);
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
        let mut app = App::with_factory(backend, SwRendererFactory);
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
            app.add_plugin(mirui::plugins::FrameRateCapPlugin::new(cap));
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
