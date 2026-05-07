use alloc::vec::Vec;

use super::{Backend, DisplayInfo, InputEvent};
use crate::types::Rect;

/// A simple framebuffer backend that owns a buffer and calls a user-provided flush callback.
pub struct FramebufBackend<F: FnMut(&[u8], &Rect)> {
    buf: Vec<u8>,
    width: u16,
    height: u16,
    flush_cb: F,
}

impl<F: FnMut(&[u8], &Rect)> FramebufBackend<F> {
    pub fn new(width: u16, height: u16, flush_cb: F) -> Self {
        let buf = alloc::vec![0u8; width as usize * height as usize * 4];
        Self {
            buf,
            width,
            height,
            flush_cb,
        }
    }
}

impl<F: FnMut(&[u8], &Rect)> Backend for FramebufBackend<F> {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: 1,
        }
    }

    fn framebuffer(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn flush(&mut self, area: &Rect) {
        (self.flush_cb)(&self.buf, area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
}
