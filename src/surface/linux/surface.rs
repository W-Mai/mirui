#![cfg(all(feature = "linux-fb", target_os = "linux"))]

use alloc::collections::VecDeque;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;

use evdev::Device;
use memmap2::{MmapMut, MmapOptions};

use super::ioctl;
use crate::cache::InspectCaches;
use crate::draw::texture::{ColorFormat, Texture};
use crate::event::input::InputEvent;
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
}

impl Default for LinuxConfig<'_> {
    fn default() -> Self {
        Self {
            fb_path: "/dev/fb0",
            input_path: None,
            overscan_inset_percent: 0,
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
    input: Option<EvdevInput>,
    queue: VecDeque<InputEvent>,
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
        let detected;
        let input_path = match cfg.input_path {
            Some(p) => Some(p),
            None => {
                detected = detect_pointer_device();
                detected.as_deref()
            }
        };
        let input = input_path.and_then(|p| match EvdevInput::open(p, width, height) {
            Ok(input) => Some(input),
            Err(err) => {
                eprintln!("mirui::linux: skipping input device {p}: {err}");
                None
            }
        });

        Ok(Self {
            _file: file,
            mmap,
            width,
            height,
            line_length,
            format,
            view_byte_offset,
            input,
            queue: VecDeque::new(),
        })
    }
}

impl InspectCaches for LinuxFbSurface {}

impl Surface for LinuxFbSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: Fixed::ONE,
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
        if let Some(ev) = self.queue.pop_front() {
            return Some(ev);
        }
        if let Some(input) = self.input.as_mut() {
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

/// Auto-detection avoids hard-coding a node number: SBC mainline
/// boards typically expose the touch panel at `event0`, but a USB
/// keyboard / power button can shift it (QEMU's `usb-tablet` lands
/// on `event1` once `usb-kbd` is plugged in).
fn detect_pointer_device() -> Option<alloc::string::String> {
    use alloc::string::String;
    use alloc::string::ToString;
    for n in 0..16 {
        let path = alloc::format!("/dev/input/event{n}");
        let Ok(device) = Device::open(&path) else {
            continue;
        };
        let Some(abs) = device.supported_absolute_axes() else {
            continue;
        };
        if abs.contains(evdev::AbsoluteAxisCode::ABS_X)
            && abs.contains(evdev::AbsoluteAxisCode::ABS_Y)
        {
            return Some(String::from(path.to_string()));
        }
    }
    None
}

/// Pumps `/dev/input/event*` into mirui input events. Uses
/// `evdev::Device` only to discover the absolute calibration ranges;
/// the runtime read goes straight to the fd via `libc::read` because
/// `evdev::Device::fetch_events` buffers internally and stops issuing
/// `read` syscalls once the kernel returns `EAGAIN`, leaving move
/// events trapped in the kernel's queue.
struct EvdevInput {
    fd: libc::c_int,
    _device: Device,
    state: PointerState,
    buffer: alloc::vec::Vec<u8>,
}

// `libc::input_event` already handles the 32-bit / 64-bit time field
// split that the kernel introduced in 5.1; re-using it avoids hand
// rolling the wrong layout on a non-`x86_64` host.
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const BTN_LEFT: u16 = 0x110;
const BTN_TOUCH: u16 = 0x14a;

/// Driver-agnostic input axes; unit tests drive `PointerState`
/// through these without an evdev device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAxis {
    Slot(u8),
    TrackingId(i32),
    AbsX(i32),
    AbsY(i32),
    Button(bool),
    /// SYN_REPORT.
    Sync,
}

/// Pointer / touch state machine. Logic is `evdev`-free so the test
/// harness can exercise it directly.
struct PointerState {
    width: u16,
    height: u16,
    abs_min_x: i32,
    abs_max_x: i32,
    abs_min_y: i32,
    abs_max_y: i32,
    slot: u8,
    last_xy: [(Fixed, Fixed); 16],
    last_down: [bool; 16],
    /// Touched slots since the last `Sync`. Bit `i` set = slot `i`'s
    /// position changed and needs a `PointerMove` once the report
    /// boundary arrives.
    dirty: u16,
}

impl PointerState {
    fn new(
        width: u16,
        height: u16,
        abs_min_x: i32,
        abs_max_x: i32,
        abs_min_y: i32,
        abs_max_y: i32,
    ) -> Self {
        Self {
            width,
            height,
            abs_min_x,
            abs_max_x,
            abs_min_y,
            abs_max_y,
            slot: 0,
            last_xy: [(Fixed::ZERO, Fixed::ZERO); 16],
            last_down: [false; 16],
            dirty: 0,
        }
    }

    fn process(&mut self, axis: InputAxis, queue: &mut VecDeque<InputEvent>) {
        match axis {
            InputAxis::Slot(s) => {
                self.slot = s.min(15);
            }
            InputAxis::TrackingId(value) => {
                let i = self.slot as usize;
                if value < 0 {
                    if self.last_down[i] {
                        let (x, y) = self.last_xy[i];
                        queue.push_back(InputEvent::PointerUp {
                            id: self.slot,
                            x,
                            y,
                        });
                        self.last_down[i] = false;
                    }
                } else if !self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerDown {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = true;
                }
            }
            InputAxis::AbsX(value) => {
                let i = self.slot as usize;
                self.last_xy[i].0 =
                    map_axis(value, self.abs_min_x, self.abs_max_x, self.width as i32);
                self.dirty |= 1 << i;
            }
            InputAxis::AbsY(value) => {
                let i = self.slot as usize;
                self.last_xy[i].1 =
                    map_axis(value, self.abs_min_y, self.abs_max_y, self.height as i32);
                self.dirty |= 1 << i;
            }
            InputAxis::Button(true) => {
                let i = self.slot as usize;
                if !self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerDown {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = true;
                }
            }
            InputAxis::Button(false) => {
                let i = self.slot as usize;
                if self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerUp {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = false;
                }
            }
            InputAxis::Sync => {
                let dirty = core::mem::take(&mut self.dirty);
                for i in 0..16 {
                    if dirty & (1 << i) == 0 {
                        continue;
                    }
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerMove { id: i as u8, x, y });
                }
            }
        }
    }
}

impl EvdevInput {
    fn open(path: &str, width: u16, height: u16) -> io::Result<Self> {
        let mut device = Device::open(path)?;
        let fd = device.as_raw_fd();
        // mirui polls input synchronously — a blocking `read` would
        // freeze rendering between events. `O_NONBLOCK` lets the
        // raw `read` return `EAGAIN` immediately when the kernel's
        // event queue is empty. SAFETY: `fd` is a valid evdev file
        // descriptor owned by `device` for the rest of this method.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let abs_info = device.get_abs_state().ok();
        let (abs_min_x, abs_max_x, abs_min_y, abs_max_y) = abs_info
            .as_ref()
            .map(|s| {
                let mt_x = &s[evdev::AbsoluteAxisCode::ABS_MT_POSITION_X.0 as usize];
                let mt_y = &s[evdev::AbsoluteAxisCode::ABS_MT_POSITION_Y.0 as usize];
                let x = &s[evdev::AbsoluteAxisCode::ABS_X.0 as usize];
                let y = &s[evdev::AbsoluteAxisCode::ABS_Y.0 as usize];
                let pick = |mt: &libc::input_absinfo, plain: &libc::input_absinfo| {
                    if mt.maximum > 0 {
                        (mt.minimum, mt.maximum)
                    } else {
                        (plain.minimum, plain.maximum)
                    }
                };
                let (xmin, xmax) = pick(mt_x, x);
                let (ymin, ymax) = pick(mt_y, y);
                (xmin, xmax, ymin, ymax)
            })
            .unwrap_or((0, width as i32, 0, height as i32));
        device.grab().ok();
        Ok(Self {
            fd,
            _device: device,
            state: PointerState::new(width, height, abs_min_x, abs_max_x, abs_min_y, abs_max_y),
            buffer: alloc::vec![0u8; 64 * core::mem::size_of::<libc::input_event>()],
        })
    }

    fn drain_into(&mut self, queue: &mut VecDeque<InputEvent>) {
        let stride = core::mem::size_of::<libc::input_event>();
        loop {
            let n =
                unsafe { libc::read(self.fd, self.buffer.as_mut_ptr().cast(), self.buffer.len()) };
            // EAGAIN/error: exit; retry next frame.
            if n <= 0 {
                return;
            }
            let count = n as usize / stride;
            for i in 0..count {
                // SAFETY: `n / stride` records bound the iteration to
                // exactly `count` complete `input_event`s already
                // copied into `buffer` by the kernel.
                let raw = unsafe {
                    let ptr = self
                        .buffer
                        .as_ptr()
                        .add(i * stride)
                        .cast::<libc::input_event>();
                    ptr.read_unaligned()
                };
                let axis = match raw.type_ {
                    EV_ABS => match raw.code {
                        ABS_MT_SLOT => Some(InputAxis::Slot(raw.value as u8)),
                        ABS_MT_TRACKING_ID => Some(InputAxis::TrackingId(raw.value)),
                        ABS_X | ABS_MT_POSITION_X => Some(InputAxis::AbsX(raw.value)),
                        ABS_Y | ABS_MT_POSITION_Y => Some(InputAxis::AbsY(raw.value)),
                        _ => None,
                    },
                    EV_KEY => {
                        if raw.code == BTN_LEFT || raw.code == BTN_TOUCH {
                            Some(InputAxis::Button(raw.value == 1))
                        } else {
                            None
                        }
                    }
                    EV_SYN if raw.code == SYN_REPORT => Some(InputAxis::Sync),
                    _ => None,
                };
                if let Some(axis) = axis {
                    self.state.process(axis, queue);
                }
            }
        }
    }
}

fn map_axis(value: i32, min: i32, max: i32, screen: i32) -> Fixed {
    let span = (max - min).max(1);
    let v = value.saturating_sub(min);
    let scaled = (v as i64 * screen as i64 / span as i64) as i32;
    Fixed::from_int(scaled.max(0).min(screen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(state: &mut PointerState, axes: &[InputAxis]) -> Vec<InputEvent> {
        let mut queue = VecDeque::new();
        for &a in axes {
            state.process(a, &mut queue);
        }
        queue.into_iter().collect()
    }

    fn st() -> PointerState {
        // 800×600 screen, evdev range matches QEMU's USB Tablet.
        PointerState::new(800, 600, 0, 32767, 0, 32767)
    }

    #[test]
    fn hover_emits_pointer_move_without_button() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(16383),
                InputAxis::AbsY(16383),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 1, "hover sync must emit a single PointerMove");
        match &events[0] {
            InputEvent::PointerMove { id, x, y } => {
                assert_eq!(*id, 0);
                // 16383 / 32767 → integer mapping, ±1 px slack.
                assert!((x.to_int() - 400).abs() <= 1);
                assert!((y.to_int() - 300).abs() <= 1);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }

    #[test]
    fn axis_without_sync_does_not_emit() {
        let mut s = st();
        let events = drain(&mut s, &[InputAxis::AbsX(100), InputAxis::AbsY(200)]);
        assert!(events.is_empty(), "no Sync = no move yet, got {events:?}");
    }

    #[test]
    fn button_press_release_emits_down_then_up() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(0),
                InputAxis::AbsY(0),
                InputAxis::Button(true),
                InputAxis::Sync,
                InputAxis::Button(false),
                InputAxis::Sync,
            ],
        );
        assert!(matches!(events[0], InputEvent::PointerDown { id: 0, .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { id: 0, .. }));
        assert!(matches!(events[2], InputEvent::PointerUp { id: 0, .. }));
    }

    #[test]
    fn drag_after_button_emits_move_per_sync() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::Button(true),
                InputAxis::AbsX(8192),
                InputAxis::Sync,
                InputAxis::AbsX(16384),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], InputEvent::PointerDown { .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { .. }));
        assert!(matches!(events[2], InputEvent::PointerMove { .. }));
    }

    #[test]
    fn multitouch_tracking_id_drives_down_up() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::Slot(1),
                InputAxis::TrackingId(42),
                InputAxis::AbsX(0),
                InputAxis::AbsY(0),
                InputAxis::Sync,
                InputAxis::TrackingId(-1),
                InputAxis::Sync,
            ],
        );
        assert!(matches!(events[0], InputEvent::PointerDown { id: 1, .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { id: 1, .. }));
        assert!(matches!(events[2], InputEvent::PointerUp { id: 1, .. }));
    }

    #[test]
    fn axis_clamps_outside_calibration_range() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(-10),
                InputAxis::AbsY(99999),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 0);
                assert_eq!(y.to_int(), 600);
            }
            _ => panic!("expected PointerMove"),
        }
    }
}
