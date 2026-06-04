#![cfg(all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"))]

use alloc::collections::VecDeque;
use std::io;
use std::os::fd::AsRawFd;

use evdev::Device;

use crate::event::input::InputEvent;

pub(super) fn detect_pointer_device() -> Option<alloc::string::String> {
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
            return Some(path.to_string());
        }
        if rel_fallback.is_none()
            && rel.is_some_and(|r| {
                r.contains(evdev::RelativeAxisCode::REL_X)
                    && r.contains(evdev::RelativeAxisCode::REL_Y)
            })
        {
            rel_fallback = Some(path.to_string());
        }
    }
    rel_fallback
}

pub(super) fn detect_keyboard_device() -> Option<alloc::string::String> {
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
            return Some(path.to_string());
        }
    }
    None
}

// `evdev::Device::fetch_events` buffers and stops at EAGAIN, trapping
// queued events; we read from the fd directly to avoid that.
pub(super) struct EvdevInput {
    fd: libc::c_int,
    _device: Device,
    state: PointerState,
    buffer: alloc::vec::Vec<u8>,
}

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

use crate::surface::input_state::{InputAxis, PointerState};

impl EvdevInput {
    pub(super) fn open_pointer(path: &str, width: u16, height: u16) -> io::Result<Self> {
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

    pub(super) fn open_keyboard(path: &str) -> io::Result<Self> {
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
        // SAFETY: `fd` owned by `device` for the duration of this fn.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(device)
    }

    pub(super) fn drain_into(&mut self, queue: &mut VecDeque<InputEvent>) {
        let stride = core::mem::size_of::<libc::input_event>();
        loop {
            let n =
                unsafe { libc::read(self.fd, self.buffer.as_mut_ptr().cast(), self.buffer.len()) };
            if n <= 0 {
                return;
            }
            let count = n as usize / stride;
            for i in 0..count {
                // SAFETY: kernel writes `count` complete `input_event`s into `buffer`.
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
                            // value=2 is auto-repeat — long-press handles it.
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

fn linux_keycode_to_mirui(code: u16) -> u32 {
    use crate::event::input::*;
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
