#![cfg(all(feature = "linux-drm", target_os = "linux"))]

use alloc::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::input::EvdevInput;
use super::scale::ScaleMode;
use crate::cache::InspectCaches;
use crate::draw::texture::{ColorFormat, Texture};
use crate::event::input::InputEvent;
use crate::surface::{DisplayInfo, FramebufferAccess, Surface};
use crate::types::{Fixed, Rect};

#[derive(Debug, Clone)]
pub struct LinuxDrmConfig<'a> {
    pub card_path: &'a str,
    /// `Some("HDMI-A-1")` to force connector; `None` picks first connected.
    pub connector_filter: Option<&'a str>,
    /// `None` → connector's preferred mode.
    pub mode: Option<(u16, u16)>,
    /// `None` → auto-detect.
    pub input_path: Option<&'a str>,
    /// Per-side inset in %, capped at 25.
    pub overscan_inset_percent: u8,
    pub scale: ScaleMode,
    pub buffer_count: u8,
}

impl Default for LinuxDrmConfig<'_> {
    fn default() -> Self {
        Self {
            card_path: "/dev/dri/card0",
            connector_filter: None,
            mode: None,
            input_path: None,
            overscan_inset_percent: 0,
            scale: ScaleMode::default(),
            buffer_count: 2,
        }
    }
}

pub struct LinuxDrmSurface {
    width: u16,
    height: u16,
    format: ColorFormat,
    scale: Fixed,
    inputs: alloc::vec::Vec<EvdevInput>,
    queue: VecDeque<InputEvent>,
    quit_flag: Arc<AtomicBool>,
}

impl LinuxDrmSurface {
    pub fn open(_cfg: LinuxDrmConfig<'_>) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "linux-drm backend not yet implemented (scaffold only)",
        ))
    }
}

impl InspectCaches for LinuxDrmSurface {}

impl Surface for LinuxDrmSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: self.scale,
            format: self.format,
        }
    }

    fn flush(&mut self, _area: &Rect) {
        // No-op for now; flip happens in `frame_end` once implemented.
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(ev) = self.queue.pop_front() {
            return Some(ev);
        }
        for input in &mut self.inputs {
            input.drain_into(&mut self.queue);
        }
        self.queue.pop_front()
    }
}

impl FramebufferAccess for LinuxDrmSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        // Placeholder — real impl returns the backbuffer's mmap slice.
        Texture::new(&mut [], 0, 0, self.format)
    }
}
