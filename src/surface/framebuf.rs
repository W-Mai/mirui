use alloc::vec::Vec;

use super::{DisplayInfo, FramebufferAccess, InputEvent, Surface, logical_from_physical};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Rect};

/// Owns a physical-pixel-sized byte buffer and calls the user flush
/// callback each frame. `width` / `height` are the **physical**
/// framebuffer size; HiDPI is opt-in via `with_scale`.
pub struct FramebufSurface<F: FnMut(&[u8], &Rect)> {
    buf: Vec<u8>,
    width: u16,
    height: u16,
    scale: Fixed,
    format: ColorFormat,
    flush_cb: F,
}

impl<F: FnMut(&[u8], &Rect)> FramebufSurface<F> {
    pub fn new(width: u16, height: u16, flush_cb: F) -> Self {
        Self::with_scale_and_format(width, height, Fixed::ONE, ColorFormat::RGBA8888, flush_cb)
    }

    pub fn with_format(width: u16, height: u16, format: ColorFormat, flush_cb: F) -> Self {
        Self::with_scale_and_format(width, height, Fixed::ONE, format, flush_cb)
    }

    /// Construct a backend that renders into a `physical_w × physical_h`
    /// byte buffer but reports `(physical / scale)` as the logical
    /// screen size — user code then writes its layout against the
    /// logical dims and the scale is applied by the render pipeline.
    pub fn with_scale(physical_w: u16, physical_h: u16, scale: Fixed, flush_cb: F) -> Self {
        Self::with_scale_and_format(
            physical_w,
            physical_h,
            scale,
            ColorFormat::RGBA8888,
            flush_cb,
        )
    }

    pub fn with_scale_and_format(
        physical_w: u16,
        physical_h: u16,
        scale: Fixed,
        format: ColorFormat,
        flush_cb: F,
    ) -> Self {
        let scale = if scale <= Fixed::ZERO {
            Fixed::ONE
        } else {
            scale
        };
        let buf =
            alloc::vec![0u8; physical_w as usize * physical_h as usize * format.bytes_per_pixel()];
        Self {
            buf,
            width: physical_w,
            height: physical_h,
            scale,
            format,
            flush_cb,
        }
    }
}

impl<F: FnMut(&[u8], &Rect)> crate::cache::InspectCaches for FramebufSurface<F> {}

impl<F: FnMut(&[u8], &Rect)> Surface for FramebufSurface<F> {
    fn display_info(&self) -> DisplayInfo {
        let (lw, lh) = logical_from_physical(self.width, self.height, self.scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale: self.scale,
            format: self.format,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn flush(&mut self, area: &Rect) {
        (self.flush_cb)(&self.buf, area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
}

impl<F: FnMut(&[u8], &Rect)> FramebufferAccess for FramebufSurface<F> {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(&mut self.buf, self.width, self.height, self.format)
    }
}
