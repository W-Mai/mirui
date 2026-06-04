#![cfg(all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"))]

mod input;

// linux-drm wins when both features are on (e.g. `--all-features` builds).
#[cfg(all(feature = "linux-fb", not(feature = "linux-drm")))]
mod ioctl;
#[cfg(all(feature = "linux-fb", not(feature = "linux-drm")))]
mod surface;

#[cfg(feature = "linux-drm")]
mod drm;

pub use crate::surface::scale::ScaleMode;

#[cfg(all(feature = "linux-fb", not(feature = "linux-drm")))]
pub use surface::{LinuxConfig, LinuxFbSurface};

#[cfg(all(feature = "linux-fb", not(feature = "linux-drm")))]
pub fn init(cfg: LinuxConfig<'_>) -> std::io::Result<LinuxFbSurface> {
    LinuxFbSurface::open(cfg)
}

#[cfg(feature = "linux-drm")]
pub use drm::{LinuxDrmConfig, LinuxDrmSurface};

#[cfg(feature = "linux-drm")]
pub fn init_drm(cfg: LinuxDrmConfig<'_>) -> std::io::Result<LinuxDrmSurface> {
    LinuxDrmSurface::open(cfg)
}
