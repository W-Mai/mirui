use alloc::string::String;

pub use wgpu::{PowerPreference, PresentMode};

/// Device selection, swapchain pacing, and color attachment configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WgpuConfig {
    /// None selects Mailbox, then Immediate, then Fifo. Explicit non-Auto modes must be supported by the surface.
    pub present_mode: Option<PresentMode>,
    /// Used only when selecting a device, not when restoring its window.
    pub power_preference: PowerPreference,
    /// Nonzero requested queue depth; the platform may clamp this hint.
    pub maximum_frame_latency: u32,
    /// Color sample count. One disables MSAA; unsupported counts return an error.
    pub samples: u32,
}

impl Default for WgpuConfig {
    fn default() -> Self {
        Self {
            present_mode: None,
            power_preference: PowerPreference::default(),
            maximum_frame_latency: 2,
            samples: if cfg!(any(target_os = "android", target_os = "ios")) {
                1
            } else {
                4
            },
        }
    }
}

/// Failure to create or configure a WGPU surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WgpuSurfaceError {
    InvalidFrameLatency,
    UnsupportedSamples(u32),
    UnsupportedPresentMode(PresentMode),
    UnsupportedSurface,
    Backend(String),
}

impl core::fmt::Display for WgpuSurfaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidFrameLatency => f.write_str("WGPU frame latency must be nonzero"),
            Self::UnsupportedSamples(count) => write!(f, "unsupported WGPU sample count: {count}"),
            Self::UnsupportedPresentMode(mode) => {
                write!(f, "unsupported WGPU present mode: {mode:?}")
            }
            Self::UnsupportedSurface => {
                f.write_str("surface lacks the required format or copy/render capabilities")
            }
            Self::Backend(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for WgpuSurfaceError {}

impl WgpuConfig {
    pub(super) fn validate(self) -> Result<(), WgpuSurfaceError> {
        if self.maximum_frame_latency == 0 {
            return Err(WgpuSurfaceError::InvalidFrameLatency);
        }
        if !matches!(self.samples, 1 | 2 | 4 | 8 | 16) {
            return Err(WgpuSurfaceError::UnsupportedSamples(self.samples));
        }
        Ok(())
    }

    pub(super) fn select_present_mode(
        self,
        modes: &[PresentMode],
    ) -> Result<PresentMode, WgpuSurfaceError> {
        match self.present_mode {
            Some(mode @ (PresentMode::AutoVsync | PresentMode::AutoNoVsync)) => Ok(mode),
            Some(mode) if modes.contains(&mode) => Ok(mode),
            Some(mode) => Err(WgpuSurfaceError::UnsupportedPresentMode(mode)),
            None => [
                PresentMode::Mailbox,
                PresentMode::Immediate,
                PresentMode::Fifo,
            ]
            .into_iter()
            .find(|mode| modes.contains(mode))
            .ok_or(WgpuSurfaceError::UnsupportedSurface),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_modes_never_silently_fallback_and_invalid_sizes_fail() {
        let mut config = WgpuConfig::default();
        assert_eq!(
            config.select_present_mode(&[PresentMode::Fifo]),
            Ok(PresentMode::Fifo)
        );
        config.present_mode = Some(PresentMode::Immediate);
        assert_eq!(
            config.select_present_mode(&[PresentMode::Fifo]),
            Err(WgpuSurfaceError::UnsupportedPresentMode(
                PresentMode::Immediate
            ))
        );
        config.present_mode = Some(PresentMode::AutoNoVsync);
        assert_eq!(
            config.select_present_mode(&[PresentMode::Fifo]),
            Ok(PresentMode::AutoNoVsync)
        );
        config.maximum_frame_latency = 0;
        assert_eq!(
            config.validate(),
            Err(WgpuSurfaceError::InvalidFrameLatency)
        );
        config.maximum_frame_latency = 1;
        for samples in [0, 3, 32] {
            config.samples = samples;
            assert_eq!(
                config.validate(),
                Err(WgpuSurfaceError::UnsupportedSamples(samples))
            );
        }
    }
}
