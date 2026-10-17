#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::ui::clipboard::{ClipboardDispatch, ClipboardRead, ClipboardWrite};

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(target_os = "macos")]
pub(crate) fn write(request: ClipboardWrite) -> ClipboardDispatch {
    let result = platform::write(request.data());
    ClipboardDispatch::Complete(request.complete(result))
}

#[cfg(target_os = "windows")]
pub(crate) fn write(
    window: &impl raw_window_handle::HasWindowHandle,
    request: ClipboardWrite,
) -> ClipboardDispatch {
    let result = platform::write(window, request.data());
    ClipboardDispatch::Complete(request.complete(result))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn read(request: ClipboardRead) -> ClipboardDispatch {
    ClipboardDispatch::Complete(request.complete(platform::read()))
}
