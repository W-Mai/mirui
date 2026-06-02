#![cfg(all(feature = "linux-drm", target_os = "linux"))]

use alloc::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use drm::Device;
use drm::buffer::{Buffer, DrmFourcc};
use drm::control::Device as ControlDevice;
use drm::control::{ClipRect, Mode, connector, crtc, framebuffer};
use drm_ffi::drm_sys::drm_vblank_seq_type::_DRM_VBLANK_RELATIVE;
use drm_ffi::wait_vblank;

use super::input::{EvdevInput, detect_keyboard_device, detect_pointer_device};
use super::scale::{ScaleMode, compute_scale};
use crate::cache::InspectCaches;
use crate::draw::texture::{ColorFormat, Texture};
use crate::event::input::InputEvent;
use crate::surface::{BackbufferPersistence, DisplayInfo, FramebufferAccess, Surface};
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

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Device for Card {}
impl ControlDevice for Card {}

pub struct LinuxDrmSurface {
    card: Card,
    #[allow(dead_code)] // kept for future atomic / multi-buffer paths
    crtc: crtc::Handle,
    #[allow(dead_code)]
    connector: connector::Handle,
    #[allow(dead_code)]
    mode: Mode,
    width: u16,
    height: u16,
    format: ColorFormat,
    scale: Fixed,
    buffers: alloc::vec::Vec<DrmBuffer>,
    inputs: alloc::vec::Vec<EvdevInput>,
    queue: VecDeque<InputEvent>,
    quit_flag: Arc<AtomicBool>,
}

struct DrmBuffer {
    fb_id: framebuffer::Handle,
    dumb: drm::control::dumbbuffer::DumbBuffer,
    // mmap kept alive for the surface's lifetime: drm-rs's `DumbMapping`
    // munmaps on Drop, which would invalidate the SwRenderer slice.
    mmap_ptr: *mut u8,
    mmap_len: usize,
}

// SAFETY: process-local mmap, no threading, surface owns munmap on Drop.
unsafe impl Send for DrmBuffer {}

impl LinuxDrmSurface {
    pub fn open(cfg: LinuxDrmConfig<'_>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(cfg.card_path)
            .map_err(|e| io::Error::new(e.kind(), alloc::format!("open {}: {e}", cfg.card_path)))?;
        let card = Card(file);

        // simpledrm / efifb / simple-framebuffer accept modeset ioctls but
        // don't scan out a new buffer — black screen + hung console.
        let driver = card
            .get_driver()
            .map_err(|e| io::Error::other(alloc::format!("get_driver: {e}")))?;
        let driver_name = driver.name().to_string_lossy();
        if matches!(
            driver_name.as_ref(),
            "simpledrm" | "simple-framebuffer" | "efifb"
        ) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                alloc::format!(
                    "driver `{driver_name}` does not support real mode setting; use `linux-fb` feature instead"
                ),
            ));
        }

        card.acquire_master_lock().map_err(|e| {
            io::Error::other(alloc::format!(
                "DRM master busy on {}: {e}; stop X/Wayland or switch to a console TTY",
                cfg.card_path
            ))
        })?;

        let res = card
            .resource_handles()
            .map_err(|e| io::Error::other(alloc::format!("resource_handles: {e}")))?;

        let coninfo: alloc::vec::Vec<connector::Info> = res
            .connectors()
            .iter()
            .filter_map(|h| card.get_connector(*h, true).ok())
            .collect();

        let connector_info = pick_connector(&coninfo, cfg.connector_filter)?;
        let mode = pick_mode(connector_info.modes(), cfg.mode)?;

        let crtc_handle = pick_crtc(&card, &res, connector_info)?;

        let (mode_w, mode_h) = mode.size();
        let inset = cfg.overscan_inset_percent.min(25) as u32;
        let width = u16::try_from(mode_w as u32 * (100 - 2 * inset) / 100).unwrap_or(mode_w);
        let height = u16::try_from(mode_h as u32 * (100 - 2 * inset) / 100).unwrap_or(mode_h);

        // DRM_FORMAT_XRGB8888 = byte-order BGRA on little-endian.
        let format = ColorFormat::BGRA8888;
        let fourcc = DrmFourcc::Xrgb8888;

        let buffer_count = cfg.buffer_count.max(1) as usize;
        let mut buffers = alloc::vec::Vec::with_capacity(buffer_count);
        for _ in 0..buffer_count {
            let mut dumb = card
                .create_dumb_buffer((mode_w.into(), mode_h.into()), fourcc, 32)
                .map_err(|e| io::Error::other(alloc::format!("create_dumb_buffer: {e}")))?;
            let fb_id = card
                .add_framebuffer(&dumb, 24, 32)
                .map_err(|e| io::Error::other(alloc::format!("add_framebuffer: {e}")))?;

            let mapping = card
                .map_dumb_buffer(&mut dumb)
                .map_err(|e| io::Error::other(alloc::format!("map_dumb_buffer: {e}")))?;
            let mmap_ptr = mapping.as_ref().as_ptr() as *mut u8;
            let mmap_len = mapping.as_ref().len();
            core::mem::forget(mapping);

            buffers.push(DrmBuffer {
                fb_id,
                dumb,
                mmap_ptr,
                mmap_len,
            });
        }

        card.set_crtc(
            crtc_handle,
            Some(buffers[0].fb_id),
            (0, 0),
            &[connector_info.handle()],
            Some(mode),
        )
        .map_err(|e| io::Error::other(alloc::format!("set_crtc: {e}")))?;

        let mut inputs = alloc::vec::Vec::new();
        let pointer_path_buf;
        let pointer_path = match cfg.input_path {
            Some(p) => Some(p),
            None => {
                pointer_path_buf = detect_pointer_device();
                pointer_path_buf.as_deref()
            }
        };
        if let Some(p) = pointer_path {
            match EvdevInput::open_pointer(p, width, height) {
                Ok(input) => inputs.push(input),
                Err(err) => eprintln!("mirui::linux-drm: skipping pointer {p}: {err}"),
            }
        }
        if let Some(p) = detect_keyboard_device() {
            match EvdevInput::open_keyboard(&p) {
                Ok(input) => inputs.push(input),
                Err(err) => eprintln!("mirui::linux-drm: skipping keyboard {p}: {err}"),
            }
        }

        let quit_flag = Arc::new(AtomicBool::new(false));
        for sig in [libc::SIGINT, libc::SIGTERM] {
            let _ = signal_hook::flag::register(sig, Arc::clone(&quit_flag));
        }

        // Paravirtual drivers report bogus mm sizes; fall back to 1.0×.
        let mm_lying = matches!(driver_name.as_ref(), "virtio_gpu" | "vmwgfx" | "qxl");
        let scale_mode = match cfg.scale {
            ScaleMode::AutoDpi { .. } if mm_lying => ScaleMode::Fixed(Fixed::ONE),
            other => other,
        };
        let (mm_w, mm_h) = connector_info.size().unwrap_or((0, 0));
        let scale = compute_scale(scale_mode, mode_w, mode_h, mm_w as u32, mm_h as u32);

        Ok(Self {
            card,
            crtc: crtc_handle,
            connector: connector_info.handle(),
            mode,
            width,
            height,
            format,
            scale,
            buffers,
            inputs,
            queue: VecDeque::new(),
            quit_flag,
        })
    }
}

fn pick_connector<'a>(
    coninfo: &'a [connector::Info],
    filter: Option<&str>,
) -> io::Result<&'a connector::Info> {
    if let Some(name_filter) = filter {
        // Match `xrandr`/`kmsprint` form: `<Interface>-<id>`, e.g. `HDMI-A-1`.
        coninfo
            .iter()
            .filter(|c| c.state() == connector::State::Connected)
            .find(|c| {
                let name = alloc::format!("{}-{}", c.interface().as_str(), c.interface_id());
                name == name_filter
            })
            .ok_or_else(|| {
                io::Error::other(alloc::format!(
                    "no connected connector matches `{name_filter}`"
                ))
            })
    } else {
        coninfo
            .iter()
            .find(|c| c.state() == connector::State::Connected)
            .ok_or_else(|| io::Error::other("no connected connector found"))
    }
}

fn pick_mode(modes: &[Mode], filter: Option<(u16, u16)>) -> io::Result<Mode> {
    if let Some((w, h)) = filter {
        modes
            .iter()
            .copied()
            .find(|m| m.size() == (w, h))
            .ok_or_else(|| io::Error::other(alloc::format!("no mode matches {w}×{h}")))
    } else {
        modes
            .first()
            .copied()
            .ok_or_else(|| io::Error::other("connector reports no modes"))
    }
}

fn pick_crtc(
    card: &Card,
    res: &drm::control::ResourceHandles,
    connector_info: &connector::Info,
) -> io::Result<crtc::Handle> {
    if let Some(enc_h) = connector_info.current_encoder()
        && let Ok(enc) = card.get_encoder(enc_h)
        && let Some(crtc_h) = enc.crtc()
    {
        return Ok(crtc_h);
    }

    for enc_h in connector_info.encoders() {
        if let Ok(enc) = card.get_encoder(*enc_h) {
            let allowed = res.filter_crtcs(enc.possible_crtcs());
            if let Some(&crtc_h) = allowed.first() {
                return Ok(crtc_h);
            }
        }
    }
    Err(io::Error::other(
        "no CRTC reachable from connector's encoders",
    ))
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

    fn flush(&mut self, area: &Rect) {
        // Paravirtual drivers (virtio_gpu/vmwgfx) need MODE_DIRTYFB to flush.
        let (x0, y0, x1, y1) = area.pixel_bounds();
        let clip = ClipRect::new(
            x0.max(0) as u16,
            y0.max(0) as u16,
            (x1.max(0) as u16).min(self.width),
            (y1.max(0) as u16).min(self.height),
        );
        let _ = self.card.dirty_framebuffer(self.buffers[0].fb_id, &[clip]);
    }

    fn frame_end(&mut self) {
        // vc4/mali/iMX implement DRM wait_vblank; fbdev compat layer doesn't.
        let _ = wait_vblank(self.card.as_fd(), _DRM_VBLANK_RELATIVE, 1, 0);
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

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Persistent
    }
}

impl FramebufferAccess for LinuxDrmSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        let buf = &self.buffers[0];
        let stride = buf.dumb.pitch() as usize;
        // SAFETY: `mmap_ptr/len` valid until self's Drop; `&mut self` excludes aliasing.
        let slice: &mut [u8] =
            unsafe { core::slice::from_raw_parts_mut(buf.mmap_ptr, buf.mmap_len) };
        let mut tex = Texture::new(slice, self.width, self.height, self.format);
        tex.stride = stride;
        tex
    }
}

impl Drop for LinuxDrmSurface {
    fn drop(&mut self) {
        for buf in core::mem::take(&mut self.buffers) {
            // SAFETY: `mmap_ptr/len` come from a successful map_dumb_buffer.
            unsafe {
                libc::munmap(buf.mmap_ptr as *mut libc::c_void, buf.mmap_len);
            }
            let _ = self.card.destroy_framebuffer(buf.fb_id);
            let _ = self.card.destroy_dumb_buffer(buf.dumb);
        }
        let _ = self.card.release_master_lock();
    }
}
