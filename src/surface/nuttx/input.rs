use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_int;
use std::ffi::CString;
use std::io;

use libc::{O_NONBLOCK, O_RDONLY, close, open, read};

use super::ioctl::*;
use crate::event::input::InputEvent;
use crate::surface::input_state::{InputAxis, PointerState};

pub(super) struct TouchInput {
    fd: c_int,
    state: PointerState,
    buf: Vec<u8>,
}

impl TouchInput {
    pub(super) fn open(path: &str, width: u16, height: u16) -> io::Result<Self> {
        let cpath = CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "touch path contains NUL"))?;
        // SAFETY: `cpath` is a valid NUL-terminated C string.
        let fd = unsafe { open(cpath.as_ptr(), O_RDONLY | O_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let state = PointerState::new(
            width,
            height,
            // NuttX touchscreen drivers report pre-calibrated x/y in pixel
            // coords (touch_point_s.x/y is i16 of pixel position, not raw
            // ADC ticks). PointerState's calibration map is a no-op when
            // min=0, max=screen_dim, scaled to full screen.
            0,
            width as i32,
            0,
            height as i32,
        );
        let buf =
            vec![0u8; TOUCH_SAMPLE_HEADER + MAX_TOUCH_POINTS * core::mem::size_of::<TouchPoint>()];
        Ok(Self { fd, state, buf })
    }

    pub(super) fn drain_into(&mut self, queue: &mut VecDeque<InputEvent>) {
        loop {
            // SAFETY: `self.buf.as_mut_ptr()` is a valid pointer to
            // `self.buf.len()` bytes; kernel writes up to `len` bytes
            // through it. Caller (mirui tick loop) is single-threaded.
            let n = unsafe { read(self.fd, self.buf.as_mut_ptr().cast(), self.buf.len()) };
            if n <= 0 {
                return;
            }
            let n = n as usize;
            if n < TOUCH_SAMPLE_HEADER {
                // circbuf only releases whole samples; short read = driver oddity.
                return;
            }
            // see ioctl::TOUCH_SAMPLE_HEADER for the npoints + padding layout.
            let npoints = i32::from_ne_bytes(
                self.buf[..4]
                    .try_into()
                    .expect("buf slice is 4 bytes by construction"),
            ) as usize;
            let count = npoints.min(MAX_TOUCH_POINTS);
            for i in 0..count {
                let off = TOUCH_SAMPLE_HEADER + i * core::mem::size_of::<TouchPoint>();
                if off + core::mem::size_of::<TouchPoint>() > n {
                    return;
                }
                // SAFETY: kernel wrote a `touch_point_s` at this offset;
                // `read_unaligned` handles potentially-misaligned access
                // (the buffer is `Vec<u8>`-aligned, not 8-aligned).
                let p: TouchPoint = unsafe {
                    core::ptr::read_unaligned(self.buf.as_ptr().add(off) as *const TouchPoint)
                };
                self.feed(p, queue);
            }
        }
    }

    /// Axis order is load-bearing: `Slot` first (routes later axes to the
    /// right contact), then position, then `TrackingId` (down/up edge),
    /// finally `Sync` (flushes the slot's dirty bit into `PointerMove`).
    fn feed(&mut self, p: TouchPoint, queue: &mut VecDeque<InputEvent>) {
        self.state.process(InputAxis::Slot(p.id), queue);
        if p.flags & TOUCH_POS_VALID != 0 {
            self.state.process(InputAxis::AbsX(p.x as i32), queue);
            self.state.process(InputAxis::AbsY(p.y as i32), queue);
        }
        if p.flags & TOUCH_DOWN != 0 {
            // `TrackingId(>=0)` is the down-edge signal. NuttX doesn't
            // expose a kernel-assigned tracking id; reusing the slot id
            // is fine because PointerState only checks sign of value.
            self.state
                .process(InputAxis::TrackingId(p.id as i32), queue);
        } else if p.flags & TOUCH_UP != 0 {
            self.state.process(InputAxis::TrackingId(-1), queue);
        }
        // Sync flushes the contact's pending PointerMove; TOUCH_MOVE needs
        // no own axis since AbsX/AbsY already set the dirty bit.
        self.state.process(InputAxis::Sync, queue);
    }
}

impl Drop for TouchInput {
    fn drop(&mut self) {
        // SAFETY: `fd` was opened by `open()` and is closed once here.
        unsafe { close(self.fd) };
    }
}

pub(super) struct KeyInput {
    fd: c_int,
    buf: Vec<u8>,
}

impl KeyInput {
    pub(super) fn open(path: &str) -> io::Result<Self> {
        let cpath = CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "kbd path contains NUL"))?;
        // SAFETY: `cpath` is a valid NUL-terminated C string.
        let fd = unsafe { open(cpath.as_ptr(), O_RDONLY | O_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // 32 events/read stays under one circbuf burst.
        let buf = vec![0u8; 32 * core::mem::size_of::<KeyboardEvent>()];
        Ok(Self { fd, buf })
    }

    pub(super) fn drain_into(&mut self, queue: &mut VecDeque<InputEvent>) {
        loop {
            // SAFETY: `self.buf.as_mut_ptr()` valid for `self.buf.len()`
            // bytes; kernel writes at most that.
            let n = unsafe { read(self.fd, self.buf.as_mut_ptr().cast(), self.buf.len()) };
            if n <= 0 {
                return;
            }
            let count = (n as usize) / core::mem::size_of::<KeyboardEvent>();
            for i in 0..count {
                let off = i * core::mem::size_of::<KeyboardEvent>();
                // SAFETY: kernel wrote `KeyboardEvent` records into the
                // buffer; `read_unaligned` handles non-4-aligned offsets.
                let ev: KeyboardEvent = unsafe {
                    core::ptr::read_unaligned(self.buf.as_ptr().add(off) as *const KeyboardEvent)
                };
                let pressed = ev.event_type == KEYBOARD_PRESS;
                queue.push_back(InputEvent::Key {
                    code: x11_keysym_to_mirui(ev.code),
                    pressed,
                });
            }
        }
    }
}

impl Drop for KeyInput {
    fn drop(&mut self) {
        // SAFETY: `fd` was opened by `open()` and is closed once here.
        unsafe { close(self.fd) };
    }
}

/// NuttX's keyboard upper half emits X11 keysyms. Unmapped keys pass
/// through as the raw keysym so user dispatch can still match literals.
fn x11_keysym_to_mirui(code: u32) -> u32 {
    use crate::event::input::*;
    match code {
        0xFF0D => KEY_RETURN,    // XK_Return
        0xFF1B => KEY_ESCAPE,    // XK_Escape
        0xFF08 => KEY_BACKSPACE, // XK_BackSpace
        0xFF50 => KEY_HOME,      // XK_Home
        0xFF51 => KEY_LEFT,      // XK_Left
        0xFF53 => KEY_RIGHT,     // XK_Right
        0xFF57 => KEY_END,       // XK_End
        0xFFFF => KEY_DELETE,    // XK_Delete
        _ => code,
    }
}
