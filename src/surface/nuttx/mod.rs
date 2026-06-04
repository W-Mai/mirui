#![cfg(all(feature = "nuttx", target_os = "nuttx"))]

// NuttX RTOS backend. Shaped like `crate::surface::linux` but the NuttX
// kernel API differs from Linux fbdev / evdev in struct layout and IOCTL
// numbers, so the device structs and constants are not shared.

mod fb;
mod input;
mod ioctl;
pub mod log;
mod signal;
mod surface;

pub use surface::{NuttxConfig, NuttxFbSurface};

pub fn init(cfg: NuttxConfig<'_>) -> std::io::Result<NuttxFbSurface> {
    NuttxFbSurface::open(cfg)
}

/// No `.M` plane suffix: multi-plane single-display boards aren't exposed in v1.
pub fn fb_path_for_display(display_index: u8) -> alloc::string::String {
    alloc::format!("/dev/fb{display_index}")
}
