pub mod backends;
pub mod canvas;
pub mod command;
pub mod factory;
pub mod font;
pub mod membrane;
pub mod mirx_convert;
pub mod mirx_frames;
pub mod painter;
pub mod partial;
pub mod path;
mod projective_fallback;
pub mod raster;
pub mod renderer;
pub mod scene;
pub mod texture;

#[cfg(feature = "sdl-gpu")]
pub use backends::sdl_gpu;
pub use backends::sw;
#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub use backends::web_canvas;
#[cfg(feature = "wgpu")]
pub use backends::wgpu;
pub use canvas::{Canvas, Paint};
pub use command::{CompositeMode, DrawCommand};
pub use factory::{RendererFactory, SwRendererFactory};
pub use projective_fallback::ProjectiveFallback;
pub use renderer::ProjectiveDrawError;
pub use renderer::Renderer;
pub use sw::SwRenderer;
#[cfg(feature = "perf")]
pub use sw::{PerfCtx, quad_perf};
