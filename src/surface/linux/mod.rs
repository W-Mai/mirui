#![cfg(all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"))]

mod input;
mod scale;

#[cfg(feature = "linux-fb")]
mod ioctl;
#[cfg(feature = "linux-fb")]
mod surface;

#[cfg(feature = "linux-drm")]
mod drm;

pub use scale::ScaleMode;

#[cfg(feature = "linux-fb")]
pub use surface::{LinuxConfig, LinuxFbSurface};

#[cfg(feature = "linux-fb")]
pub fn init(cfg: LinuxConfig<'_>) -> std::io::Result<LinuxFbSurface> {
    LinuxFbSurface::open(cfg)
}

#[cfg(feature = "linux-drm")]
pub use drm::{LinuxDrmConfig, LinuxDrmSurface};

#[cfg(feature = "linux-drm")]
pub fn init_drm(cfg: LinuxDrmConfig<'_>) -> std::io::Result<LinuxDrmSurface> {
    LinuxDrmSurface::open(cfg)
}
