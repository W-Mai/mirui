pub mod framebuf;
#[cfg(feature = "sdl")]
pub mod sdl;

use crate::draw::texture::Texture;
use crate::types::{CoordTransform, Fixed, Rect};

/// Display information
pub struct DisplayInfo {
    pub width: u16,
    pub height: u16,
    pub scale: Fixed,
    pub format: crate::draw::texture::ColorFormat,
}

impl DisplayInfo {
    #[inline]
    pub fn transform(&self) -> CoordTransform {
        CoordTransform::new(self.width, self.height, self.scale)
    }
}

/// Input event from the platform
#[derive(Clone, Debug)]
pub enum InputEvent {
    Touch { x: Fixed, y: Fixed },
    TouchMove { x: Fixed, y: Fixed },
    Release { x: Fixed, y: Fixed },
    Key { code: u32, pressed: bool },
    Quit,
}

/// Platform backend trait — abstracts display + input.
///
/// Does **not** assume the backend has a CPU-accessible framebuffer;
/// GPU-only backends (SDL GPU, wgpu, VG-Lite, …) implement this trait
/// without `FramebufferAccess`. CPU raster backends additionally
/// implement [`FramebufferAccess`] to expose their framebuffer to
/// `SwDrawBackendFactory`.
pub trait Backend {
    fn display_info(&self) -> DisplayInfo;

    /// Present a region of the backing display after rendering.
    fn flush(&mut self, area: &Rect);

    fn poll_event(&mut self) -> Option<InputEvent>;

    fn screen_rect(&self) -> Rect {
        let info = self.display_info();
        Rect::new(0, 0, info.width, info.height)
    }
}

/// A [`Backend`] that exposes a CPU-accessible framebuffer as a [`Texture`].
///
/// `SwDrawBackendFactory` blanket-implements `RendererFactory` for any
/// backend satisfying this trait. GPU backends should not implement it —
/// their factories access GPU resources through backend-specific
/// methods instead.
pub trait FramebufferAccess: Backend {
    fn framebuffer(&mut self) -> Texture<'_>;
}
