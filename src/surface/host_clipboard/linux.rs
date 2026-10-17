use std::ffi::CStr;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

use crate::ui::clipboard::{
    ClipboardDispatch, ClipboardError, ClipboardRead, ClipboardResponse, ClipboardWrite,
};

use super::x11::Bridge;

#[derive(Default)]
pub(crate) struct Clipboard {
    bridge: Option<Bridge>,
}

#[cfg(feature = "sdl")]
pub(crate) fn is_x11(window: &impl HasDisplayHandle) -> bool {
    window
        .display_handle()
        .is_ok_and(|handle| matches!(handle.as_raw(), RawDisplayHandle::Xlib(_)))
}

impl Clipboard {
    fn bridge(&mut self, window: &impl HasDisplayHandle) -> Result<&mut Bridge, ClipboardError> {
        if self.bridge.as_ref().is_some_and(Bridge::finished) {
            self.bridge = None;
        }
        if self.bridge.is_none() {
            let handle = window
                .display_handle()
                .map_err(|_| ClipboardError::Unavailable)?;
            let RawDisplayHandle::Xlib(display) = handle.as_raw() else {
                return Err(ClipboardError::Unsupported);
            };
            let pointer = display.display.ok_or(ClipboardError::Unavailable)?;
            let xlib = x11_dl::xlib::Xlib::open().map_err(|_| ClipboardError::Unavailable)?;
            // The display is borrowed from the live window only for this call.
            let name = unsafe { (xlib.XDisplayString)(pointer.as_ptr().cast()) };
            if name.is_null() {
                return Err(ClipboardError::Unavailable);
            }
            // XDisplayString is a NUL-terminated string owned by that display.
            let name = unsafe { CStr::from_ptr(name) }
                .to_str()
                .map_err(|_| ClipboardError::InvalidData)?
                .to_owned();
            // Selection ownership is display-wide, independent of the window's screen.
            let screen = unsafe { (xlib.XDefaultScreen)(pointer.as_ptr().cast()) };
            let screen = usize::try_from(screen).map_err(|_| ClipboardError::InvalidData)?;
            self.bridge = Some(Bridge::new(name, screen)?);
        }
        self.bridge.as_mut().ok_or(ClipboardError::Unavailable)
    }

    pub(crate) fn write(
        &mut self,
        window: &impl HasDisplayHandle,
        request: ClipboardWrite,
    ) -> ClipboardDispatch {
        match self.bridge(window) {
            Ok(bridge) => bridge.write(request),
            Err(error) => ClipboardDispatch::Complete(request.complete(Err(error))),
        }
    }

    pub(crate) fn read(
        &mut self,
        window: &impl HasDisplayHandle,
        request: ClipboardRead,
    ) -> ClipboardDispatch {
        match self.bridge(window) {
            Ok(bridge) => bridge.read(request),
            Err(error) => ClipboardDispatch::Complete(request.complete(Err(error))),
        }
    }

    pub(crate) fn poll(&mut self) -> Option<ClipboardResponse> {
        self.bridge.as_mut()?.poll()
    }
}
