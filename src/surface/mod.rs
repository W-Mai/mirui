pub mod framebuf;
#[cfg(feature = "sdl")]
pub mod sdl;
#[cfg(feature = "sdl-gpu")]
pub mod sdl_gpu;
#[cfg(feature = "std")]
pub mod slow;

use crate::draw::texture::Texture;
use crate::types::{Fixed, Rect, Viewport};

/// Display information reported by a backend. `width` / `height` are in
/// **logical pixels** — the units user code writes `Dimension::px(…)` in.
/// Physical framebuffer size is `Surface::physical_size()`.
pub struct DisplayInfo {
    pub width: u16,
    pub height: u16,
    pub scale: Fixed,
    pub format: crate::draw::texture::ColorFormat,
}

impl DisplayInfo {
    #[inline]
    pub fn viewport(&self) -> Viewport {
        let phys_w = (Fixed::from(self.width) * self.scale).to_int().max(0) as u16;
        let phys_h = (Fixed::from(self.height) * self.scale).to_int().max(0) as u16;
        Viewport::new(phys_w, phys_h, self.scale)
    }
}

pub use crate::event::input::{InputEvent, KEY_HW_BUTTON_0, KEY_ROTARY_PRESS};

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
/// `SwRendererFactory`.
pub trait Surface: crate::cache::InspectCaches {
    fn display_info(&self) -> DisplayInfo;

    /// Present the given **physical-pixel** region of the backing surface.
    /// `App` is responsible for converting a logical dirty rect to physical
    /// before calling this; driver-side code treats `area` as raw device
    /// coordinates / buffer offsets.
    fn flush(&mut self, area: &Rect);

    fn poll_event(&mut self) -> Option<InputEvent>;

    /// Full logical-pixel screen rect.
    fn screen_rect(&self) -> Rect {
        let info = self.display_info();
        Rect::new(0, 0, info.width, info.height)
    }

    /// Physical pixel dimensions of the backing surface. Default derives
    /// from `display_info()`; backends that store physical dims directly
    /// should override to skip the multiply-and-round.
    fn physical_size(&self) -> (u32, u32) {
        let info = self.display_info();
        let pw = (Fixed::from(info.width) * info.scale).to_int().max(0) as u32;
        let ph = (Fixed::from(info.height) * info.scale).to_int().max(0) as u32;
        (pw, ph)
    }

    /// Defaults to [`BackbufferPersistence::Persistent`]; swap-chain
    /// GPU backends override to [`BackbufferPersistence::Transient`].
    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Persistent
    }
}

/// Convert a backend-private physical pixel size to logical via `scale`.
/// Used by the bundled backends' `display_info()` to publish logical
/// dims without touching internal buffers sized in physical pixels.
#[inline]
pub(crate) fn logical_from_physical(phys_w: u16, phys_h: u16, scale: Fixed) -> (u16, u16) {
    if scale <= Fixed::ZERO {
        return (phys_w, phys_h);
    }
    let lw = (Fixed::from(phys_w) / scale).to_int().max(0) as u16;
    let lh = (Fixed::from(phys_h) / scale).to_int().max(0) as u16;
    (lw, lh)
}

/// A [`Surface`] that exposes a CPU-accessible framebuffer as a [`Texture`].
///
/// `SwRendererFactory` blanket-implements `RendererFactory` for any
/// backend satisfying this trait. GPU backends should not implement it —
/// their factories access GPU resources through backend-specific
/// methods instead.
pub trait FramebufferAccess: Surface {
    fn framebuffer(&mut self) -> Texture<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOpBackend;
    impl crate::cache::InspectCaches for NoOpBackend {}
    impl Surface for NoOpBackend {
        fn display_info(&self) -> DisplayInfo {
            DisplayInfo {
                width: 1,
                height: 1,
                scale: Fixed::ONE,
                format: crate::draw::texture::ColorFormat::RGBA8888,
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
