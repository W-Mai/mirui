use super::SurfaceSizeError;
use sdl2::video::{FullscreenType, Window};

pub(crate) fn supported(window: &Window) -> bool {
    window.fullscreen_state() == FullscreenType::Off
        && matches!(
            window.subsystem().current_video_driver(),
            "cocoa" | "x11" | "wayland" | "windows"
        )
}

pub(crate) fn request(
    window: &mut Window,
    size: (u16, u16),
) -> Result<(u16, u16), SurfaceSizeError> {
    if !supported(window) || window.is_maximized() {
        return Err(SurfaceSizeError::Restricted);
    }
    window
        .set_size(u32::from(size.0), u32::from(size.1))
        .map_err(|_| SurfaceSizeError::Backend)?;
    let actual = window.size();
    Ok((
        u16::try_from(actual.0).map_err(|_| SurfaceSizeError::Restricted)?,
        u16::try_from(actual.1).map_err(|_| SurfaceSizeError::Restricted)?,
    ))
}
