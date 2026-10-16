use crate::render::texture::{ColorFormat, Texture};
use crate::render::{PlaneLayout, PlaneRequirements, RenderError};
use crate::types::PhysicalRect;
use sdl2::render::{Canvas, RenderTarget};

pub(super) fn texture<T: RenderTarget>(
    canvas: &Canvas<T>,
    area: PhysicalRect,
) -> Result<Texture<'static>, RenderError> {
    let mut texture = Texture::owned(area.width(), area.height(), ColorFormat::RGBA8888)?;
    let mut pixels = texture.pixels_mut()?;
    let stride = pixels.stride();
    rgba8(canvas, area, pixels.bytes_mut(), stride)?;
    Ok(texture)
}

pub(super) fn rgba8<T: RenderTarget>(
    canvas: &Canvas<T>,
    area: PhysicalRect,
    bytes: &mut [u8],
    stride: usize,
) -> Result<(), RenderError> {
    if area.is_empty() {
        return Ok(());
    }
    let layout = PlaneLayout::new(
        area.width(),
        area.height(),
        ColorFormat::RGBA8888,
        stride,
        PlaneRequirements::CPU,
    )
    .map_err(|_| RenderError::InvalidTexture)?;
    if bytes.len() < layout.required_bytes() {
        return Err(RenderError::InvalidTexture);
    }
    let pitch = i32::try_from(stride).map_err(|_| RenderError::InvalidTexture)?;
    let rect = sdl2_sys::SDL_Rect {
        x: i32::from(area.x()),
        y: i32::from(area.y()),
        w: i32::from(area.width()),
        h: i32::from(area.height()),
    };
    // The canvas owns a live renderer; the checked destination contains every pitched row.
    let result = unsafe {
        sdl2_sys::SDL_RenderReadPixels(
            canvas.raw(),
            &rect,
            sdl2_sys::SDL_PixelFormatEnum::SDL_PIXELFORMAT_RGBA32 as u32,
            bytes.as_mut_ptr().cast(),
            pitch,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(RenderError::BackendFailure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

    #[test]
    fn reads_into_prepared_rows_without_rust_allocations_or_padding_writes() {
        let surface =
            sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(17, 39, 83, 127));
        canvas.clear();
        let area = PhysicalRect::new(1, 1, 2, 2).unwrap();
        let mut bytes = [0xa5; 32];
        rgba8(&canvas, area, &mut bytes, 16).unwrap();
        let allocations = tracked_allocations(|| {
            for _ in 0..3 {
                rgba8(&canvas, area, &mut bytes, 16).unwrap();
            }
        });
        assert_eq!(allocations, 0);
        for row in bytes.chunks_exact(16) {
            assert_eq!(&row[..8], &[17, 39, 83, 127, 17, 39, 83, 127]);
            assert_eq!(&row[8..], &[0xa5; 8]);
        }
        let snapshot = texture(&canvas, area).unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 0));
        canvas.clear();
        drop(canvas);
        assert_eq!(snapshot.width(), 2);
        assert_eq!(snapshot.height(), 2);
        assert_eq!(
            snapshot.read_pixels().unwrap().bytes(),
            &[17, 39, 83, 127].repeat(4)
        );
    }

    #[test]
    fn invalid_storage_is_rejected_without_writing_and_empty_reads_are_noops() {
        let surface =
            sdl2::surface::Surface::new(2, 2, sdl2::pixels::PixelFormatEnum::RGBA32).unwrap();
        let canvas = surface.into_canvas().unwrap();
        let area = PhysicalRect::new(0, 0, 2, 2).unwrap();
        for stride in [0, 7, 12, usize::MAX] {
            let mut bytes = [0xa5; 16];
            assert_eq!(
                rgba8(&canvas, area, &mut bytes, stride),
                Err(RenderError::InvalidTexture)
            );
            assert_eq!(bytes, [0xa5; 16]);
        }
        assert_eq!(rgba8(&canvas, PhysicalRect::EMPTY, &mut [], 0), Ok(()));
    }
}
