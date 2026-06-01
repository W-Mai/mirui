#![cfg(all(feature = "linux-fb", target_os = "linux"))]

use alloc::collections::VecDeque;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

        Ok(Self {
            _file: file,
            mmap,
            width,
            height,
            line_length,
            format,
            view_byte_offset,
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
///
/// Prefers absolute (touch panel / `usb-tablet`) over relative (USB
/// mouse) — when both exist the absolute device is the natural
/// touch surface and an EV_REL stream layered on top of slot 0
/// would race for the cursor.
fn detect_pointer_device() -> Option<alloc::string::String> {
    use alloc::string::String;
    use alloc::string::ToString;
    let mut rel_fallback: Option<String> = None;
    for n in 0..16 {
        let path = alloc::format!("/dev/input/event{n}");
        let Ok(device) = Device::open(&path) else {
            continue;
        };
        let abs = device.supported_absolute_axes();
        let rel = device.supported_relative_axes();
        if abs.is_some_and(|a| {
            a.contains(evdev::AbsoluteAxisCode::ABS_X) && a.contains(evdev::AbsoluteAxisCode::ABS_Y)
        }) {
            return Some(String::from(path.to_string()));
        }
        if rel_fallback.is_none()
            && rel.is_some_and(|r| {
                r.contains(evdev::RelativeAxisCode::REL_X)
                    && r.contains(evdev::RelativeAxisCode::REL_Y)
            })
        {
            rel_fallback = Some(String::from(path.to_string()));
        }
    }
    rel_fallback
}

/// "Keyboard" = device with `KEY_A` and no `ABS_X`/`REL_X` —
/// excludes touch panels with hard buttons.
fn detect_keyboard_device() -> Option<alloc::string::String> {
    use alloc::string::String;
    use alloc::string::ToString;
    for n in 0..16 {
        let path = alloc::format!("/dev/input/event{n}");
        let Ok(device) = Device::open(&path) else {
            continue;
        };
        let has_letters = device
            .supported_keys()
            .is_some_and(|k| k.contains(evdev::KeyCode::KEY_A));
        let has_pointer = device
            .supported_absolute_axes()
            .is_some_and(|a| a.contains(evdev::AbsoluteAxisCode::ABS_X))
            || device
                .supported_relative_axes()
                .is_some_and(|r| r.contains(evdev::RelativeAxisCode::REL_X));
        if has_letters && !has_pointer {
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
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;
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
    RelX(i32),
    RelY(i32),
    RelWheel(i32),
    RelHWheel(i32),
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
    /// Coalesced per Sync — drivers emit several `REL_WHEEL` per detent.
    wheel_dx: i32,
    wheel_dy: i32,
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
            wheel_dx: 0,
            wheel_dy: 0,
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
            InputAxis::RelX(delta) => {
                let nx = (self.last_xy[0].0 + Fixed::from_int(delta))
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int(self.width as i32 - 1));
                self.last_xy[0].0 = nx;
                self.dirty |= 1;
            }
            InputAxis::RelY(delta) => {
                let ny = (self.last_xy[0].1 + Fixed::from_int(delta))
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int(self.height as i32 - 1));
                self.last_xy[0].1 = ny;
                self.dirty |= 1;
            }
            InputAxis::RelWheel(delta) => self.wheel_dy = self.wheel_dy.saturating_add(delta),
            InputAxis::RelHWheel(delta) => self.wheel_dx = self.wheel_dx.saturating_add(delta),
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
                let dx = core::mem::take(&mut self.wheel_dx);
                let dy = core::mem::take(&mut self.wheel_dy);
                if dx != 0 || dy != 0 {
                    let (x, y) = self.last_xy[0];
                    queue.push_back(InputEvent::Wheel {
                        dx: Fixed::from_int(dx),
                        dy: Fixed::from_int(dy),
                        x,
                        y,
                    });
                }
            }
        }
    }
}

impl EvdevInput {
    fn open_pointer(path: &str, width: u16, height: u16) -> io::Result<Self> {
        let mut device = Self::nonblocking_device(path)?;
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
        let fd = device.as_raw_fd();
        Ok(Self {
            fd,
            _device: device,
            state: PointerState::new(width, height, abs_min_x, abs_max_x, abs_min_y, abs_max_y),
            buffer: alloc::vec![0u8; 64 * core::mem::size_of::<libc::input_event>()],
        })
    }

    /// `PointerState` here is vestigial — keyboard drains never feed
    /// it Abs/Rel/Sync, just sharing one struct keeps `drain_into`
    /// uniform.
    fn open_keyboard(path: &str) -> io::Result<Self> {
        let device = Self::nonblocking_device(path)?;
        let fd = device.as_raw_fd();
        Ok(Self {
            fd,
            _device: device,
            state: PointerState::new(0, 0, 0, 0, 0, 0),
            buffer: alloc::vec![0u8; 64 * core::mem::size_of::<libc::input_event>()],
        })
    }

    fn nonblocking_device(path: &str) -> io::Result<Device> {
        let device = Device::open(path)?;
        let fd = device.as_raw_fd();
        // mirui polls input synchronously — a blocking `read` would
        // freeze rendering between events. SAFETY: `fd` is a valid
        // evdev file descriptor owned by `device` for the rest of
        // this method.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(device)
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
                    EV_REL => match raw.code {
                        REL_X => Some(InputAxis::RelX(raw.value)),
                        REL_Y => Some(InputAxis::RelY(raw.value)),
                        REL_WHEEL => Some(InputAxis::RelWheel(raw.value)),
                        REL_HWHEEL => Some(InputAxis::RelHWheel(raw.value)),
                        _ => None,
                    },
                    EV_KEY => {
                        if raw.code == BTN_LEFT || raw.code == BTN_TOUCH {
                            Some(InputAxis::Button(raw.value == 1))
                        } else if raw.value == 0 || raw.value == 1 {
                            // value=2 is auto-repeat; mirui handles
                            // repeats via long-press, ignore here.
                            queue.push_back(InputEvent::Key {
                                code: linux_keycode_to_mirui(raw.code),
                                pressed: raw.value == 1,
                            });
                            None
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

/// `linux/input-event-codes.h` `KEY_*` → mirui SDL-style codes;
/// unmapped keys pass through as the raw linux scancode so user
/// dispatch tables can still match.
fn linux_keycode_to_mirui(code: u16) -> u32 {
    use crate::event::input::*;
    // linux/input-event-codes.h
    const KEY_ESC: u16 = 1;
    const KEY_BACKSPACE_LX: u16 = 14;
    const KEY_ENTER: u16 = 28;
    const KEY_HOME_LX: u16 = 102;
    const KEY_LEFT_LX: u16 = 105;
    const KEY_RIGHT_LX: u16 = 106;
    const KEY_END_LX: u16 = 107;
    const KEY_DELETE_LX: u16 = 111;
    match code {
        KEY_ESC => KEY_ESCAPE,
        KEY_BACKSPACE_LX => KEY_BACKSPACE,
        KEY_ENTER => KEY_RETURN,
        KEY_LEFT_LX => KEY_LEFT,
        KEY_RIGHT_LX => KEY_RIGHT,
        KEY_HOME_LX => KEY_HOME,
        KEY_END_LX => KEY_END,
        KEY_DELETE_LX => KEY_DELETE,
        _ => u32::from(code),
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
    fn rel_xy_accumulates_into_pointer_move() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelX(50),
                InputAxis::RelY(30),
                InputAxis::Sync,
                InputAxis::RelX(20),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 50);
                assert_eq!(y.to_int(), 30);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
        match &events[1] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 70);
                assert_eq!(y.to_int(), 30);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }

    #[test]
    fn rel_xy_clamps_to_screen_bounds() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelX(-100),
                InputAxis::RelY(-100),
                InputAxis::Sync,
                InputAxis::RelX(10000),
                InputAxis::RelY(10000),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 0);
                assert_eq!(y.to_int(), 0);
            }
            _ => panic!(),
        }
        match &events[1] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 799);
                assert_eq!(y.to_int(), 599);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn wheel_coalesces_into_single_event_per_sync() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelWheel(1),
                InputAxis::RelWheel(2),
                InputAxis::RelHWheel(-1),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            InputEvent::Wheel { dx, dy, .. } => {
                assert_eq!(dx.to_int(), -1);
                assert_eq!(dy.to_int(), 3);
            }
            other => panic!("expected Wheel, got {other:?}"),
        }
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
