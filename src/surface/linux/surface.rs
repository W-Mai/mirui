#![cfg(all(feature = "linux-fb", target_os = "linux"))]

use alloc::collections::VecDeque;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use memmap2::{MmapMut, MmapOptions};

use super::input::{EvdevInput, detect_keyboard_device, detect_pointer_device};
use super::ioctl;
use crate::cache::InspectCaches;
use crate::draw::texture::{ColorFormat, Texture};
use crate::event::input::InputEvent;
use crate::surface::scale::{ScaleMode, compute_scale};
use crate::surface::{DisplayInfo, FramebufferAccess, Surface};
use crate::types::{Fixed, Rect};

/// Configuration for [`super::init`]. `fb_path` defaults to
/// `/dev/fb0`; `input_path` defaults to `None`, which triggers
/// auto-detection of an absolute pointer.
#[derive(Debug, Clone)]
pub struct LinuxConfig<'a> {
    pub fb_path: &'a str,
    /// `Some(path)` opens that device verbatim. `None` scans
    /// `/dev/input/event0..15` for the first node that reports
    /// `ABS_X` + `ABS_Y`; if none match the surface comes up
    /// display-only.
    pub input_path: Option<&'a str>,
    /// Inset the view by N% on every side, centred on the panel.
    /// 0 = full panel. Capped at 25%.
    pub overscan_inset_percent: u8,
    /// Override the auto-detected DPI scale. `None` reads
    /// `var.width` / `var.height` (mm) and divides actual DPI by
    /// `baseline_dpi` (default 96 = legacy desktop). Drivers that
    /// report 0 mm fall through to scale 1.0.
    pub scale: ScaleMode,
}

impl Default for LinuxConfig<'_> {
    fn default() -> Self {
        Self {
            fb_path: "/dev/fb0",
            input_path: None,
            overscan_inset_percent: 0,
            scale: ScaleMode::default(),
        }
    }
}

pub struct LinuxFbSurface {
    _file: std::fs::File,
    mmap: MmapMut,
    width: u16,
    height: u16,
    line_length: usize,
    format: ColorFormat,
    view_byte_offset: usize,
    scale: Fixed,
    inputs: alloc::vec::Vec<EvdevInput>,
    queue: VecDeque<InputEvent>,
    quit_flag: Arc<AtomicBool>,
}

impl LinuxFbSurface {
    pub fn open(cfg: LinuxConfig<'_>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(cfg.fb_path)
            .map_err(|e| io::Error::new(e.kind(), alloc::format!("open {}: {e}", cfg.fb_path)))?;
        let fd = file.as_raw_fd();

        // SAFETY: `fd` outlives the ioctl call (kept by `file`); both
        // ioctls only fill caller-allocated structs.
        let var = unsafe { ioctl::fbioget_vscreeninfo(fd)? };
        let fix = unsafe { ioctl::fbioget_fscreeninfo(fd)? };

        let format = format_from_var(&var)?;
        let bytes_per_pixel = format.bytes_per_pixel();
        let line_length = fix.line_length as usize;
        let fb_width = u16::try_from(var.xres).map_err(invalid_data)?;
        let fb_height = u16::try_from(var.yres).map_err(invalid_data)?;

        // Cap at 25% so a misconfigured value can't collapse the view.
        let inset = cfg.overscan_inset_percent.min(25) as u32;
        let width = u16::try_from(fb_width as u32 * (100 - 2 * inset) / 100).unwrap_or(fb_width);
        let height = u16::try_from(fb_height as u32 * (100 - 2 * inset) / 100).unwrap_or(fb_height);
        let off_x = (fb_width - width) / 2;
        let off_y = (fb_height - height) / 2;
        let view_byte_offset = off_y as usize * line_length + off_x as usize * bytes_per_pixel;

        // `/dev/fb0` is a char device — `fstat` returns size 0 and
        // `MmapMut::map_mut(&file)` would mmap nothing. `smem_len` is
        // the driver-authoritative framebuffer size.
        let map_len = fix.smem_len as usize;
        if map_len == 0 {
            return Err(invalid_data("driver reports smem_len = 0"));
        }
        // SAFETY: mmap requires the fd to stay valid for the lifetime
        // of the mapping; `_file` keeps it alive.
        let mmap = unsafe { MmapOptions::new().len(map_len).map_mut(&file)? };

        // Open the framebuffer first so a missing / locked input
        // device degrades to display-only rather than failing the
        // whole surface — the screen is the more disruptive thing to
        // lose. Input errors are reported once on stderr and forgotten.
        let mut inputs = alloc::vec::Vec::new();
        let detected_pointer;
        let pointer_path = match cfg.input_path {
            Some(p) => Some(p),
            None => {
                detected_pointer = detect_pointer_device();
                detected_pointer.as_deref()
            }
        };
        if let Some(p) = pointer_path {
            match EvdevInput::open_pointer(p, width, height) {
                Ok(input) => inputs.push(input),
                Err(err) => eprintln!("mirui::linux: skipping pointer {p}: {err}"),
            }
        }
        // Keyboard on its own fd; Quit comes from SIGINT, not Esc,
        // so no editing-key remap goes here.
        if let Some(p) = detect_keyboard_device() {
            match EvdevInput::open_keyboard(&p) {
                Ok(input) => inputs.push(input),
                Err(err) => eprintln!("mirui::linux: skipping keyboard {p}: {err}"),
            }
        }

        // poll_event turns the flag into a Quit on the next tick.
        // Register failures are non-fatal — a hosting harness may
        // already trap SIGINT.
        let quit_flag = Arc::new(AtomicBool::new(false));
        for sig in [libc::SIGINT, libc::SIGTERM] {
            let _ = signal_hook::flag::register(sig, Arc::clone(&quit_flag));
        }

        let scale = compute_scale(cfg.scale, fb_width, fb_height, var.width, var.height);

        Ok(Self {
            _file: file,
            mmap,
            width,
            height,
            line_length,
            format,
            view_byte_offset,
            scale,
            inputs,
            queue: VecDeque::new(),
            quit_flag,
        })
    }
}

impl InspectCaches for LinuxFbSurface {}

impl Surface for LinuxFbSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: self.scale,
            format: self.format,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn flush(&mut self, _area: &Rect) {
        // Direct mmap; no staging copy / present here.
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if self.quit_flag.swap(false, Ordering::Relaxed) {
            return Some(InputEvent::Quit);
        }
        if let Some(ev) = self.queue.pop_front() {
            return Some(ev);
        }
        for input in &mut self.inputs {
            input.drain_into(&mut self.queue);
        }
        self.queue.pop_front()
    }
}

impl FramebufferAccess for LinuxFbSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        let mut tex = Texture::new(
            &mut self.mmap[self.view_byte_offset..],
            self.width,
            self.height,
            self.format,
        );
        // line_length covers both driver scanline padding and the
        // overscan border the renderer must skip.
        tex.stride = self.line_length;
        tex
    }
}

fn format_from_var(var: &ioctl::FbVarScreeninfo) -> io::Result<ColorFormat> {
    match var.bits_per_pixel {
        // BGRX panel: fbdev signals via red.offset > blue.offset.
        32 if var.red.offset > var.blue.offset => Ok(ColorFormat::BGRA8888),
        32 => Ok(ColorFormat::RGBA8888),
        16 => Ok(ColorFormat::RGB565),
        bpp => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            alloc::format!("unsupported framebuffer depth: {bpp} bpp"),
        )),
    }
}

fn invalid_data<E: core::fmt::Display>(err: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, alloc::format!("{err}"))
}
