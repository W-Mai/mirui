use alloc::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::fb::FbDevice;
use super::input::{KeyInput, TouchInput};
use super::log::{error, warn};
use super::{fb_path_for_display, signal};
use crate::cache::InspectCaches;
use crate::draw::texture::Texture;
use crate::event::input::InputEvent;
use crate::surface::scale::{ScaleMode, compute_scale};
use crate::surface::{DisplayInfo, FramebufferAccess, Surface};
use crate::types::{Fixed, Rect};

#[derive(Debug, Clone)]
pub struct NuttxConfig<'a> {
    pub fb_path: Option<&'a str>,
    pub touch_path: Option<&'a str>,
    pub keyboard_path: Option<&'a str>,
    pub display_index: u8,
    pub overscan_inset_percent: u8,
    /// `AutoDpi` falls back to `Fixed::ONE` here: NuttX doesn't report panel mm.
    pub scale: ScaleMode,
    /// PAN every frame. virtio-gpu needs it (its vsync_loop only flushes on
    /// a queued paninfo); SPI LCD drivers never consume paninfo, so PAN there
    /// fills the ring and any later `FBIO_WAITFORVSYNC` blocks forever.
    /// `None` auto-detects by pixel size: ≥32-bit → on, narrower → off.
    pub use_paninfo: Option<bool>,
}

impl<'a> Default for NuttxConfig<'a> {
    fn default() -> Self {
        Self {
            fb_path: None,
            touch_path: Some("/dev/input0"),
            keyboard_path: None,
            display_index: 0,
            overscan_inset_percent: 0,
            scale: ScaleMode::default(),
            use_paninfo: None,
        }
    }
}

pub struct NuttxFbSurface {
    fb: FbDevice,
    width: u16,
    height: u16,
    line_length: usize,
    view_byte_offset: usize,
    scale: Fixed,
    touch: Option<TouchInput>,
    keyboard: Option<KeyInput>,
    queue: VecDeque<InputEvent>,
    quit_flag: Arc<AtomicBool>,
}

impl NuttxFbSurface {
    pub fn open(cfg: NuttxConfig<'_>) -> io::Result<Self> {
        let derived;
        let fb_path = match cfg.fb_path {
            Some(p) => p,
            None => {
                derived = fb_path_for_display(cfg.display_index);
                derived.as_str()
            }
        };
        let fb = FbDevice::open(fb_path, cfg.use_paninfo)?;

        let inset = cfg.overscan_inset_percent.min(25) as u32;
        let width = u16::try_from(fb.xres as u32 * (100 - 2 * inset) / 100).unwrap_or(fb.xres);
        let height = u16::try_from(fb.yres as u32 * (100 - 2 * inset) / 100).unwrap_or(fb.yres);
        let bytes_per_pixel = fb.format.bytes_per_pixel();
        let off_x = (fb.xres - width) / 2;
        let off_y = (fb.yres - height) / 2;
        let view_byte_offset = off_y as usize * fb.stride + off_x as usize * bytes_per_pixel;

        let touch = match cfg.touch_path {
            Some(p) => match TouchInput::open(p, width, height) {
                Ok(t) => Some(t),
                Err(err) => {
                    warn!("mirui::nuttx: skipping touch {p}: {err}");
                    None
                }
            },
            None => None,
        };

        let keyboard = match cfg.keyboard_path {
            Some(p) => match KeyInput::open(p) {
                Ok(k) => Some(k),
                Err(err) => {
                    warn!("mirui::nuttx: skipping keyboard {p}: {err}");
                    None
                }
            },
            None => None,
        };

        let quit_flag = Arc::new(AtomicBool::new(false));
        if let Err(err) = signal::install(&quit_flag) {
            error!("mirui::nuttx: sigaction install failed: {err}");
        }

        let scale = compute_scale(cfg.scale, fb.xres, fb.yres, 0, 0);

        Ok(Self {
            fb,
            width,
            height,
            line_length: 0,
            view_byte_offset,
            scale,
            touch,
            keyboard,
            queue: VecDeque::new(),
            quit_flag,
        }
        .finalize_stride())
    }

    fn finalize_stride(mut self) -> Self {
        self.line_length = self.fb.stride;
        self
    }
}

impl InspectCaches for NuttxFbSurface {}

impl Surface for NuttxFbSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: self.scale,
            format: self.fb.format,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn flush(&mut self, area: &Rect) {
        let (x0, y0, x1, y1) = area.pixel_bounds();
        let x = x0.max(0) as u16;
        let y = y0.max(0) as u16;
        let w = (x1.max(0) as u16).saturating_sub(x).min(self.width);
        let h = (y1.max(0) as u16).saturating_sub(y).min(self.height);
        if w == 0 || h == 0 {
            return;
        }
        self.fb.flush(x, y, w, h);
    }

    fn frame_end(&mut self) {
        // PAN every frame — see `NuttxConfig::use_paninfo` for why.
        self.fb.flush(0, 0, self.fb.xres, self.fb.yres);
        self.fb.wait_vsync();
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if self.quit_flag.swap(false, Ordering::Relaxed) {
            return Some(InputEvent::Quit);
        }
        if let Some(ev) = self.queue.pop_front() {
            return Some(ev);
        }
        if let Some(t) = &mut self.touch {
            t.drain_into(&mut self.queue);
        }
        if let Some(k) = &mut self.keyboard {
            k.drain_into(&mut self.queue);
        }
        self.queue.pop_front()
    }
}

impl FramebufferAccess for NuttxFbSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        // SAFETY: `fbmem` came from FBIOGET_PLANEINFO at `open`-time and
        // stays valid for the FbDevice's lifetime; `fblen` is the exact
        // length the kernel reported. The `&mut self` borrow rules out
        // any other code grabbing a second slice into the same memory
        // while this one is live.
        let slice: &mut [u8] = unsafe {
            core::slice::from_raw_parts_mut(
                self.fb.fbmem.add(self.view_byte_offset),
                self.fb.fblen - self.view_byte_offset,
            )
        };
        let mut tex = Texture::new(slice, self.width, self.height, self.fb.format);
        tex.stride = self.line_length;
        tex
    }
}
