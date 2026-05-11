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

/// Does the backbuffer survive `flush()`?
///
/// CPU raster backends are [`Persistent`]; swap-chain GPU backends
/// (SDL accelerated / wgpu / Web canvas) are [`Transient`]. `App::run`
/// picks dirty-only vs. full-frame rendering based on this.
///
/// [`Persistent`]: Self::Persistent
/// [`Transient`]: Self::Transient
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackbufferPersistence {
    Persistent,
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

    /// Defaults to [`BackbufferPersistence::Persistent`]; swap-chain
    /// GPU backends override to [`BackbufferPersistence::Transient`].
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
