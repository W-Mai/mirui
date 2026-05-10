use alloc::vec::Vec;

use super::{Backend, DisplayInfo, FramebufferAccess, InputEvent};
use crate::draw::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Rect};

/// A simple framebuffer backend that owns a buffer and calls a user-provided flush callback.
pub struct FramebufBackend<F: FnMut(&[u8], &Rect)> {
    buf: Vec<u8>,
    width: u16,
    height: u16,
    format: ColorFormat,
    flush_cb: F,
}

impl<F: FnMut(&[u8], &Rect)> FramebufBackend<F> {
    pub fn new(width: u16, height: u16, flush_cb: F) -> Self {
        let buf = alloc::vec![0u8; width as usize * height as usize * 4];
        Self {
            buf,
            width,
            height,
            format: ColorFormat::ARGB8888,
            flush_cb,
        }
    }

    pub fn with_format(width: u16, height: u16, format: ColorFormat, flush_cb: F) -> Self {
        let buf = alloc::vec![0u8; width as usize * height as usize * format.bytes_per_pixel()];
        Self {
            buf,
            width,
            height,
            format,
            flush_cb,
        }
    }
}

impl<F: FnMut(&[u8], &Rect)> Backend for FramebufBackend<F> {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: Fixed::ONE,
            format: self.format,
        }
    }

    fn flush(&mut self, area: &Rect) {
        (self.flush_cb)(&self.buf, area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
}

impl<F: FnMut(&[u8], &Rect)> FramebufferAccess for FramebufBackend<F> {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(&mut self.buf, self.width, self.height, self.format)
    }
}
