pub mod canvas;
pub mod command;
pub mod font;
pub mod membrane;
pub mod painter;
pub mod partial;
pub mod path;
pub(crate) mod raster;
pub mod renderer;
#[cfg(feature = "sdl-gpu")]
pub mod sdl_gpu;
pub mod sw;
pub mod texture;
#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub mod web_canvas;
#[cfg(feature = "wgpu")]
pub mod wgpu_render;

pub use canvas::Canvas;
pub use command::DrawCommand;
pub use renderer::Renderer;
pub use sw::SwRenderer;
#[cfg(feature = "perf")]
pub use sw::{PerfCtx, quad_perf};
