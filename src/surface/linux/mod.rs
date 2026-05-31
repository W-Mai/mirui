#![cfg(all(feature = "linux-fb", target_os = "linux"))]

//! Linux fbdev + evdev backend. Opens `/dev/fb0`, mmaps the
//! framebuffer, and pumps `/dev/input/event*` into mirui's
//! `InputEvent` queue.

mod ioctl;
mod surface;

pub use surface::{LinuxConfig, LinuxFbSurface};

pub fn init(cfg: LinuxConfig<'_>) -> std::io::Result<LinuxFbSurface> {
    LinuxFbSurface::open(cfg)
}
