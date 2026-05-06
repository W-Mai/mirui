use alloc::vec::Vec;

use super::{Backend, DisplayInfo, InputEvent};

/// A simple framebuffer backend that owns a buffer and calls a user-provided flush callback.
pub struct FramebufBackend<F: FnMut(&[u8])> {
    buf: Vec<u8>,
    width: u16,
    height: u16,
    flush_cb: F,
}

impl<F: FnMut(&[u8])> FramebufBackend<F> {
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

impl<F: FnMut(&[u8])> Backend for FramebufBackend<F> {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
        }
    }

    fn framebuffer(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn flush(&mut self) {
        (self.flush_cb)(&self.buf);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
}
