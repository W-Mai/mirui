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

pub mod demos;
pub mod prelude {
    pub use mirui::prelude::*;
}

use mirui::app::{App, RendererFactory};
use mirui::ecs::Entity;
use mirui::surface::Surface;

pub struct SetupGeneric<'a, B: Surface, F: RendererFactory<B>> {
    pub app: &'a mut App<B, F>,
}

// `web-canvas` overrides the desktop backends on wasm32; the loader
// hands us a `<canvas>` element and we treat the title / size as
// hints rather than window-creation parameters.
#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
mod backend {
    use super::*;
    use mirui::draw::web_canvas::WebCanvasRendererFactory;
    use mirui::surface::web_canvas::WebCanvasSurface;
    use wasm_bindgen::JsCast;

    pub type ActiveSurface = WebCanvasSurface;
    pub type ActiveFactory = WebCanvasRendererFactory;

    pub fn build_app(_title: &str, _w: u16, _h: u16) -> App<ActiveSurface, ActiveFactory> {
        let canvas = web_sys::window()
            .expect("window")
            .document()
            .expect("document")
            .get_element_by_id("mirui")
            .expect("canvas element with id=\"mirui\"")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element is not <canvas>");
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

/// Active-backend setup passed to gallery demos.
pub type Setup<'a> = SetupGeneric<'a, backend::ActiveSurface, backend::ActiveFactory>;

/// Run a demo on the selected backend. Returns on wasm32 so the
/// browser keeps driving frames.
pub fn run<F>(title: &str, w: u16, h: u16, build: F)
where
    F: FnOnce(&mut Setup<'_>) -> Entity,
{
    let mut app = backend::build_app(title, w, h);
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
