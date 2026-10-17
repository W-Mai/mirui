use std::ffi::CStr;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::{
    Connection, Proxy,
    backend::{Backend, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::ui::clipboard::{
    ClipboardDispatch, ClipboardError, ClipboardRead, ClipboardResponse, ClipboardWrite,
};

use super::x11::Bridge;

#[derive(Default)]
pub(crate) struct Clipboard {
    bridge: Option<Bridge>,
    wayland: Option<super::wayland::Bridge>,
    completed: Option<ClipboardResponse>,
}

#[cfg(feature = "sdl")]
pub(crate) fn is_native(window: &impl HasDisplayHandle) -> bool {
    window.display_handle().is_ok_and(|handle| {
        matches!(
            handle.as_raw(),
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Wayland(_)
        )
    })
}

impl Clipboard {
    /// The window and display must outlive this clipboard or a call to `detach`.
    pub(crate) unsafe fn attach(&mut self, window: &(impl HasDisplayHandle + HasWindowHandle)) {
        self.detach();
        let Ok(display) = window.display_handle() else {
            return;
        };
        let Ok(window) = window.window_handle() else {
            return;
        };
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) =
            (display.as_raw(), window.as_raw())
        else {
            return;
        };
        // The caller retains both native objects until this private bridge is dropped.
        let target =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), window.surface.as_ptr().cast()) };
        if let Ok(target) = target {
            // Guest mode owns only this bridge's queue and proxies, never the host display.
            let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
            self.wayland = Some(super::wayland::Bridge::new(
                Connection::from_backend(backend),
                target,
            ));
        }
    }

    pub(crate) fn detach(&mut self) {
        if let Some(mut bridge) = self.wayland.take()
            && let Some(response) = bridge.close()
        {
            self.completed = Some(response);
        }
    }

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
        if self.completed.is_some() {
            return ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Capacity)));
        }
        if let Some(bridge) = &mut self.wayland {
            return bridge.write(request);
        }
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
        if self.completed.is_some() {
            return ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Capacity)));
        }
        if let Some(bridge) = &mut self.wayland {
            return bridge.read(request);
        }
        match self.bridge(window) {
            Ok(bridge) => bridge.read(request),
            Err(error) => ClipboardDispatch::Complete(request.complete(Err(error))),
        }
    }

    pub(crate) fn poll(&mut self) -> Option<ClipboardResponse> {
        self.completed
            .take()
            .or_else(|| self.wayland.as_mut().and_then(super::wayland::Bridge::poll))
            .or_else(|| self.bridge.as_mut().and_then(Bridge::poll))
    }
}
