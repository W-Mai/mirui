//! Concrete renderer implementations. Each backend owns its surface
//! API: `sw` is the always-on CPU rasterizer; the rest are feature
//! gated. `scene` is also always-on but draws into a Scene op stream
//! instead of pixels — same Renderer trait, different target.

pub mod scene;
pub mod sw;

#[cfg(feature = "sdl-gpu")]
pub mod sdl_gpu;

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub mod web_canvas;

#[cfg(feature = "wgpu")]
pub mod wgpu;
