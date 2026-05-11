pub mod framebuf;
#[cfg(feature = "sdl")]
pub mod sdl;
#[cfg(feature = "sdl-gpu")]
pub mod sdl_gpu;

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

/// Does the backbuffer retain the previous frame's content at the start
/// of the next render?
///
/// CPU raster backends own their framebuffer bytes and are naturally
/// [`Persistent`]. GPU backends whose presentation goes through a swap
/// chain (SDL accelerated / wgpu surface / Web canvas without
/// `preserveDrawingBuffer`) default to [`Transient`] — the back buffer
/// after `flush()` is undefined until the next full frame rewrites it.
///
/// `App::run` reads this once at startup and picks full-frame render
/// vs. dirty-only render accordingly, without backends having to
/// implement their own offscreen-composite dance.
///
/// [`Persistent`]: Self::Persistent
/// [`Transient`]: Self::Transient
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackbufferPersistence {
    /// Backbuffer survives `flush()` — App::run can do dirty-only redraws.
    Persistent,
    /// Backbuffer is undefined after `flush()` — App::run must redraw
    /// every frame.
    Transient,
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

    /// Backbuffer behaviour across `flush()` calls. Defaults to
    /// [`Persistent`] so every existing CPU backend stays correct without
    /// any code change. GPU backends override to [`Transient`].
    ///
    /// [`Persistent`]: BackbufferPersistence::Persistent
    /// [`Transient`]: BackbufferPersistence::Transient
    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Persistent
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

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOpBackend;
    impl Backend for NoOpBackend {
        fn display_info(&self) -> DisplayInfo {
            DisplayInfo {
                width: 1,
                height: 1,
                scale: Fixed::ONE,
                format: crate::draw::texture::ColorFormat::ARGB8888,
            }
        }
        fn flush(&mut self, _area: &Rect) {}
        fn poll_event(&mut self) -> Option<InputEvent> {
            None
        }
    }

    #[test]
    fn default_persistence_is_persistent() {
        let b = NoOpBackend;
        assert_eq!(b.persistence(), BackbufferPersistence::Persistent);
    }
}
