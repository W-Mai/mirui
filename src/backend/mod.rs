pub mod framebuf;
#[cfg(feature = "sdl")]
pub mod sdl;

use crate::types::{Fixed, Rect};

/// Display information
pub struct DisplayInfo {
    pub width: u16,
    pub height: u16,
    pub scale: u16,
}

/// Input event from the platform
#[derive(Clone, Debug)]
pub enum InputEvent {
    Touch { x: i32, y: i32 },
    TouchMove { x: i32, y: i32 },
    Release { x: i32, y: i32 },
    Key { code: u32, pressed: bool },
    Quit,
}

/// Platform backend trait — abstracts display + input
pub trait Backend {
    /// Get display info
    fn display_info(&self) -> DisplayInfo;

    /// Get a mutable reference to the framebuffer (RGBA8888)
    fn framebuffer(&mut self) -> &mut [u8];

    /// Flush a region of the framebuffer to the display
    fn flush(&mut self, area: &Rect);

    /// Poll for input events (non-blocking, returns None when no events)
    fn poll_event(&mut self) -> Option<InputEvent>;

    /// Full screen rect helper
    fn screen_rect(&self) -> Rect {
        let info = self.display_info();
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(info.width as i32),
            h: Fixed::from_int(info.height as i32),
        }
    }
}
