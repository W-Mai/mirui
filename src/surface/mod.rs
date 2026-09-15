#[cfg(any(test, all(feature = "web-canvas", target_arch = "wasm32")))]
pub(crate) mod backbuffer_invalidation;
pub mod framebuf;
#[cfg(any(
    all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"),
    all(feature = "nuttx", target_os = "nuttx"),
))]
pub(crate) mod input_state;
#[cfg(all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"))]
pub mod linux;
pub(crate) mod mirror;
#[cfg(all(feature = "nuttx", target_os = "nuttx"))]
pub mod nuttx;
#[cfg(any(
    all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"),
    all(feature = "nuttx", target_os = "nuttx"),
))]
pub(crate) mod scale;
#[cfg(feature = "sdl")]
pub mod sdl;
#[cfg(any(feature = "sdl", feature = "sdl-gpu"))]
pub(crate) mod sdl_events;
#[cfg(feature = "sdl-gpu")]
pub mod sdl_gpu;
#[cfg(feature = "std")]
pub mod slow;
#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub mod web_canvas;
#[cfg(feature = "wgpu")]
pub mod wgpu_surface;

use crate::render::texture::Texture;
use crate::types::{Fixed, PhysicalRect, Rect, Viewport};

/// Display information reported by a backend. `width` / `height` are in
/// **logical pixels** — the units user code writes `Dimension::px(…)` in.
/// Physical framebuffer size is `Surface::physical_size()`.
pub struct DisplayInfo {
    pub width: u16,
    pub height: u16,
    pub scale: Fixed,
    pub format: crate::render::texture::ColorFormat,
}

impl DisplayInfo {
    #[inline]
    pub fn viewport(&self) -> Viewport {
        let phys_w = saturating_u16((Fixed::from(self.width) * self.scale).to_int());
        let phys_h = saturating_u16((Fixed::from(self.height) * self.scale).to_int());
        Viewport::new(phys_w, phys_h, self.scale)
    }
}

pub use crate::input::event::input::{InputEvent, KEY_HW_BUTTON_0, KEY_ROTARY_PRESS};

/// Does the backbuffer survive `flush()`?
///
/// CPU raster and Web Canvas backends are [`Persistent`] while their backing
/// stores remain valid; swap-chain GPU backends are [`Transient`]. `App::run`
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
pub trait Surface: crate::core::cache::InspectCaches {
    fn display_info(&self) -> DisplayInfo;

    /// Authoritative logical-to-physical mapping for this surface.
    fn viewport(&self) -> Viewport {
        let info = self.display_info();
        let (physical_width, physical_height) = self.physical_size();
        Viewport::new(
            saturating_u16_from_u32(physical_width),
            saturating_u16_from_u32(physical_height),
            info.scale,
        )
    }

    /// Present the given **physical-pixel** region of the backing surface.
    /// `App` is responsible for converting a logical dirty rect to physical
    /// before calling this; driver-side code treats `area` as raw device
    /// coordinates / buffer offsets.
    fn flush(&mut self, area: PhysicalRect);

    /// Called before the frame's first `flush`. Default no-op.
    fn begin_flush(&mut self) {}

    /// Called after the frame's last `flush`. Backends with a swap
    /// chain (e.g. SDL `canvas.present()`) commit here so vsync waits
    /// once per frame, not once per `flush(area)`.
    fn end_flush(&mut self) {}

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

    /// Called at the end of every `App::tick`, even on empty frames.
    fn frame_end(&mut self) {}

    /// Default 1 skips dirty mirroring.
    fn buffer_count(&self) -> usize {
        1
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
    let lw = saturating_u16((Fixed::from(phys_w) / scale).to_int());
    let lh = saturating_u16((Fixed::from(phys_h) / scale).to_int());
    (lw, lh)
}

#[inline]
pub(crate) fn saturating_u16(value: i32) -> u16 {
    value.clamp(0, i32::from(u16::MAX)) as u16
}

#[inline]
pub(crate) const fn saturating_u16_from_u32(value: u32) -> u16 {
    if value > u16::MAX as u32 {
        u16::MAX
    } else {
        value as u16
    }
}

/// A [`Surface`] that exposes a CPU-accessible framebuffer as a [`Texture`].
///
/// `SwRendererFactory` blanket-implements `RendererFactory` for any
/// backend satisfying this trait. GPU backends should not implement it —
/// their factories access GPU resources through backend-specific
/// methods instead.
pub trait FramebufferAccess: Surface {
    fn framebuffer(&mut self) -> Texture<'_>;

    /// Index 0 is active; rest are inactives in rotation order.
    fn all_buffers(&mut self) -> alloc::vec::Vec<Texture<'_>> {
        alloc::vec![self.framebuffer()]
    }

    fn advance(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOpBackend;
    impl crate::core::cache::InspectCaches for NoOpBackend {}
    impl Surface for NoOpBackend {
        fn display_info(&self) -> DisplayInfo {
            DisplayInfo {
                width: 1,
                height: 1,
                scale: Fixed::ONE,
                format: crate::render::texture::ColorFormat::RGBA8888,
            }
        }
        fn flush(&mut self, _area: PhysicalRect) {}
        fn poll_event(&mut self) -> Option<InputEvent> {
            None
        }
    }

    #[test]
    fn default_persistence_is_persistent() {
        let b = NoOpBackend;
        assert_eq!(b.persistence(), BackbufferPersistence::Persistent);
    }

    #[test]
    fn logical_dimensions_saturate_instead_of_wrapping() {
        assert_eq!(
            logical_from_physical(u16::MAX, u16::MAX, Fixed::from_ratio(1, 2)),
            (u16::MAX, u16::MAX)
        );
        assert_eq!(saturating_u16(-1), 0);
        assert_eq!(saturating_u16(i32::MAX), u16::MAX);
        assert_eq!(saturating_u16_from_u32(u32::MAX), u16::MAX);
    }
}
