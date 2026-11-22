use core::fmt;

use crate::types::{Fixed, Viewport};

/// Accelerated renderer selection. Explicit GL variants never fall back to another driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdlGpuDriver {
    /// Let SDL select an accelerated renderer; no GL multisampling request is made.
    Auto,
    /// Desktop OpenGL. Zero disables MSAA; values greater than one request at least this many samples.
    OpenGl { samples: u8 },
    /// OpenGL ES 2. Zero disables MSAA; values greater than one request at least this many samples.
    OpenGles2 { samples: u8 },
}

/// SDL window behavior; desktop fullscreen does not change the display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdlWindowMode {
    Windowed { resizable: bool },
    FullscreenDesktop,
}

/// Window and renderer configuration. Defaults to windowed OpenGL with 4× MSAA and vsync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdlGpuConfig {
    pub driver: SdlGpuDriver,
    pub window: SdlWindowMode,
    pub vsync: bool,
}

impl Default for SdlGpuConfig {
    fn default() -> Self {
        Self {
            driver: SdlGpuDriver::OpenGl { samples: 4 },
            window: SdlWindowMode::Windowed { resizable: false },
            vsync: true,
        }
    }
}

/// SDL initialization or output geometry failure.
#[derive(Debug)]
pub enum SdlGpuError {
    InvalidConfig(&'static str),
    UnsupportedDriver(SdlGpuDriver),
    Backend(String),
}

impl fmt::Display for SdlGpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => write!(f, "invalid SDL GPU configuration: {reason}"),
            Self::UnsupportedDriver(driver) => write!(f, "SDL renderer unavailable: {driver:?}"),
            Self::Backend(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for SdlGpuError {}

pub(super) fn backend_error(operation: &str, error: impl fmt::Display) -> SdlGpuError {
    SdlGpuError::Backend(format!("{operation}: {error}"))
}

impl SdlGpuDriver {
    pub(super) fn name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::OpenGl { .. } => Some("opengl"),
            Self::OpenGles2 { .. } => Some("opengles2"),
        }
    }

    pub(super) fn select(
        self,
        names: impl Iterator<Item = Result<&'static str, SdlGpuError>>,
    ) -> Result<Option<u32>, SdlGpuError> {
        let Some(name) = self.name() else {
            return Ok(None);
        };
        for (index, candidate) in names.enumerate() {
            if candidate? == name {
                return Ok(Some(index as u32));
            }
        }
        Err(SdlGpuError::UnsupportedDriver(self))
    }

    pub(super) fn validate(self) -> Result<(), SdlGpuError> {
        match self {
            Self::OpenGl { samples: 1 } | Self::OpenGles2 { samples: 1 } => Err(
                SdlGpuError::InvalidConfig("MSAA samples must be zero or greater than one"),
            ),
            _ => Ok(()),
        }
    }

    pub(super) fn configure_gl(self) -> Result<(), SdlGpuError> {
        use sdl2::sys::SDL_GLattr::*;
        // SDL attributes are process-wide; reset before selecting a new context.
        unsafe { sdl2::sys::SDL_GL_ResetAttributes() };
        let (profile, major, minor, samples) = match self {
            Self::Auto => return Ok(()),
            Self::OpenGl { samples } => (0, 2, 1, samples),
            Self::OpenGles2 { samples } => (
                sdl2::sys::SDL_GLprofile::SDL_GL_CONTEXT_PROFILE_ES as i32,
                2,
                0,
                samples,
            ),
        };
        for (attribute, value) in [
            (SDL_GL_CONTEXT_PROFILE_MASK, profile),
            (SDL_GL_CONTEXT_MAJOR_VERSION, major),
            (SDL_GL_CONTEXT_MINOR_VERSION, minor),
            (SDL_GL_MULTISAMPLEBUFFERS, i32::from(samples > 0)),
            (SDL_GL_MULTISAMPLESAMPLES, i32::from(samples)),
        ] {
            // SDL takes enum/integer values and retains no Rust references.
            if unsafe { sdl2::sys::SDL_GL_SetAttribute(attribute, value) } != 0 {
                return Err(backend_error("SDL_GL_SetAttribute", sdl2::get_error()));
            }
        }
        Ok(())
    }

    pub(super) fn verify_gl(self) -> Result<(), SdlGpuError> {
        let samples = match self {
            Self::Auto => return Ok(()),
            Self::OpenGl { samples } | Self::OpenGles2 { samples } => samples,
        };
        let (buffers, actual) = gl_multisampling()?;
        if (samples == 0 && buffers != 0)
            || (samples > 0 && (buffers == 0 || actual < i32::from(samples)))
        {
            return Err(backend_error(
                "SDL MSAA",
                format!("requested {samples} samples, got {buffers} buffers with {actual} samples"),
            ));
        }
        Ok(())
    }
}

pub(super) fn driver_names() -> impl Iterator<Item = Result<&'static str, SdlGpuError>> {
    (0..unsafe { sdl2::sys::SDL_GetNumRenderDrivers() }).map(|index| {
        let mut info = core::mem::MaybeUninit::uninit();
        // Read driver names without decoding SDL's extensible pixel-format list.
        if unsafe { sdl2::sys::SDL_GetRenderDriverInfo(index, info.as_mut_ptr()) } != 0 {
            return Err(backend_error("SDL_GetRenderDriverInfo", sdl2::get_error()));
        }
        // SDL owns this static driver name; the structure was initialized above.
        let info = unsafe { info.assume_init() };
        unsafe { core::ffi::CStr::from_ptr(info.name) }
            .to_str()
            .map_err(|error| backend_error("SDL driver name", error))
    })
}

fn gl_multisampling() -> Result<(i32, i32), SdlGpuError> {
    use sdl2::sys::SDL_GLattr::*;
    let read = |attribute| {
        let mut value = 0;
        // Called immediately after SDL creates the renderer's current context.
        if unsafe { sdl2::sys::SDL_GL_GetAttribute(attribute, &mut value) } != 0 {
            return Err(backend_error("SDL_GL_GetAttribute", sdl2::get_error()));
        }
        Ok(value)
    };
    Ok((
        read(SDL_GL_MULTISAMPLEBUFFERS)?,
        read(SDL_GL_MULTISAMPLESAMPLES)?,
    ))
}

pub(super) fn window_is_single_sample(renderer: &str) -> bool {
    match renderer {
        // SDL texture targets have no multisample attachments. Keep drawing to
        // the window when its context uses MSAA or cannot report its state.
        "opengl" | "opengles" | "opengles2" => {
            gl_multisampling().is_ok_and(|(buffers, _)| buffers == 0)
        }
        _ => true,
    }
}

pub(super) fn viewport(logical: (u32, u32), physical: (u32, u32)) -> Result<Viewport, SdlGpuError> {
    let dimension = |value| {
        u16::try_from(value)
            .ok()
            .filter(|value| *value != 0)
            .ok_or(SdlGpuError::InvalidConfig(
                "window and output dimensions must fit nonzero u16",
            ))
    };
    let (lw, lh) = (dimension(logical.0)?, dimension(logical.1)?);
    let (pw, ph) = (dimension(physical.0)?, dimension(physical.1)?);
    let across = u64::from(pw) * u64::from(lh);
    let down = u64::from(ph) * u64::from(lw);
    if across.abs_diff(down) > u64::from(lw.max(lh)) {
        return Err(SdlGpuError::InvalidConfig(
            "output must use uniform scaling",
        ));
    }
    let scale = Fixed::from_ratio(i32::from(pw), i32::from(lw));
    if scale <= Fixed::ZERO {
        return Err(SdlGpuError::InvalidConfig(
            "output scale is below Fixed precision",
        ));
    }
    Ok(Viewport::new(pw, ph, scale).with_logical_size(lw, lh))
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/config.rs"]
mod tests;
