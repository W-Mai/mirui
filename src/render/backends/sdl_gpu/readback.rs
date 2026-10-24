use crate::render::texture::{ColorFormat, Texture};
use crate::render::{PlaneLayout, PlaneRequirements, RenderError, TargetAlpha};
use crate::types::PhysicalRect;
use sdl2::render::{Canvas, RenderTarget};

pub(super) fn edit<T: RenderTarget>(
    device: &super::Device<T>,
    region: crate::render::renderer::FallbackRegion,
    bytes: &mut [u8],
    alpha: TargetAlpha,
    body: &mut dyn FnMut(&mut Texture<'_>) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    let area = region.rect();
    let stride = region.stride_bytes();
    let mut texture = Texture::new(bytes, area.width(), area.height(), ColorFormat::RGBA8888)?
        .with_stride(stride)?;
    texture.alpha_mode = crate::render::texture::AlphaMode::Blend;
    {
        let mut pixels = texture.pixels_mut()?;
        rgba8(&*device.canvas()?, area, pixels.bytes_mut(), stride)?;
        convert_alpha(
            pixels.bytes_mut(),
            area.width(),
            area.height(),
            stride,
            alpha,
            TargetAlpha::Straight,
        )?;
    }
    // Device access ends before the callback can read another source on it.
    body(&mut texture)?;
    if (texture.width(), texture.height(), texture.format())
        != (area.width(), area.height(), ColorFormat::RGBA8888)
    {
        return Err(RenderError::InvalidTexture);
    }
    let mut pixels = texture.pixels_mut()?;
    let stride = pixels.stride();
    convert_alpha(
        pixels.bytes_mut(),
        area.width(),
        area.height(),
        stride,
        TargetAlpha::Straight,
        alpha,
    )?;
    let mut canvas = device.canvas()?;
    let mut uploaded = device
        .creator
        .create_texture_streaming(
            sdl2::pixels::PixelFormatEnum::RGBA32,
            u32::from(area.width()),
            u32::from(area.height()),
        )
        .map_err(|_| RenderError::BackendFailure)?;
    uploaded
        .update(None, pixels.bytes(), stride)
        .map_err(|_| RenderError::BackendFailure)?;
    uploaded.set_blend_mode(sdl2::render::BlendMode::None);
    let previous_clip = canvas.clip_rect();
    canvas.set_clip_rect(None);
    let result = canvas
        .copy(
            &uploaded,
            None,
            Some(sdl2::rect::Rect::new(
                i32::from(area.x()),
                i32::from(area.y()),
                u32::from(area.width()),
                u32::from(area.height()),
            )),
        )
        .map_err(|_| RenderError::BackendFailure);
    canvas.set_clip_rect(previous_clip);
    result
}

pub(super) fn region_into<T: RenderTarget>(
    canvas: &Canvas<T>,
    area: PhysicalRect,
    source_origin: (i32, i32),
    destination: &mut Texture<'_>,
    alpha: TargetAlpha,
) -> Result<(), RenderError> {
    crate::render::renderer::with_rgba8_readback(
        area,
        source_origin,
        destination,
        |area, pixels| {
            let stride = pixels.stride();
            rgba8(canvas, area, pixels.bytes_mut(), stride)?;
            convert_alpha(
                pixels.bytes_mut(),
                area.width(),
                area.height(),
                stride,
                alpha,
                TargetAlpha::Straight,
            )
        },
    )
}

pub(super) fn texture<T: RenderTarget>(
    canvas: &Canvas<T>,
    area: PhysicalRect,
    alpha: TargetAlpha,
) -> Result<Texture<'static>, RenderError> {
    let mut texture = Texture::owned(area.width(), area.height(), ColorFormat::RGBA8888)?;
    let mut pixels = texture.pixels_mut()?;
    let stride = pixels.stride();
    rgba8(canvas, area, pixels.bytes_mut(), stride)?;
    convert_alpha(
        pixels.bytes_mut(),
        area.width(),
        area.height(),
        stride,
        alpha,
        TargetAlpha::Straight,
    )?;
    texture.alpha_mode = crate::render::texture::AlphaMode::Blend;
    Ok(texture)
}

pub(super) fn convert_alpha(
    bytes: &mut [u8],
    width: u16,
    height: u16,
    stride: usize,
    from: TargetAlpha,
    to: TargetAlpha,
) -> Result<(), RenderError> {
    if from == to {
        return Ok(());
    }
    let layout = PlaneLayout::new(
        width,
        height,
        ColorFormat::RGBA8888,
        stride,
        PlaneRequirements::CPU,
    )
    .map_err(|_| RenderError::InvalidTexture)?;
    if bytes.len() < layout.minimum_bytes() {
        return Err(RenderError::InvalidTexture);
    }
    for y in 0..usize::from(height) {
        for pixel in bytes[y * stride..][..usize::from(width) * 4].chunks_exact_mut(4) {
            let converted = from.convert_rgba8(to, [pixel[0], pixel[1], pixel[2], pixel[3]]);
            pixel.copy_from_slice(&converted);
        }
    }
    Ok(())
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
    if bytes.len() < layout.minimum_bytes() {
        return Err(RenderError::InvalidTexture);
    }
    let pitch = i32::try_from(stride).map_err(|_| RenderError::InvalidTexture)?;
    let rect = sdl2_sys::SDL_Rect {
        x: i32::from(area.x()),
        y: i32::from(area.y()),
        w: i32::from(area.width()),
        h: i32::from(area.height()),
    };
    if bytes.len() < layout.required_bytes() {
        let last = usize::from(area.height() - 1) * stride;
        if area.height() > 1 {
            read_rows(
                canvas,
                sdl2_sys::SDL_Rect {
                    h: rect.h - 1,
                    ..rect
                },
                &mut bytes[..last],
                pitch,
            )?;
        }
        return read_rows(
            canvas,
            sdl2_sys::SDL_Rect {
                y: rect.y + rect.h - 1,
                h: 1,
                ..rect
            },
            &mut bytes[last..],
            i32::from(area.width()) * 4,
        );
    }
    read_rows(canvas, rect, bytes, pitch)
}

fn read_rows<T: RenderTarget>(
    canvas: &Canvas<T>,
    rect: sdl2_sys::SDL_Rect,
    bytes: &mut [u8],
    pitch: i32,
) -> Result<(), RenderError> {
    // rgba8 validates every pitched row; a short final row uses its packed width.
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
    fn edits_reuse_borrowed_storage_and_release_device_access_before_callbacks() {
        let mut canvas = sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(32, 64, 16, 128));
        canvas.clear();
        let prior_clip = sdl2::rect::Rect::new(0, 0, 1, 1);
        canvas.set_clip_rect(prior_clip);
        let creator = canvas.texture_creator();
        let device = super::super::Device::new(canvas, creator).unwrap();
        let region = crate::render::renderer::FallbackRegion::from_parts(1, 1, 2, 2, 16);
        let mut bytes = [0xa5; 32];
        let address = bytes.as_ptr();
        let mut source = [0u8; 4];
        let allocations = tracked_allocations(|| {
            edit(
                &device,
                region,
                &mut bytes,
                TargetAlpha::Premultiplied,
                &mut |texture| {
                    assert_eq!(texture.read_pixels()?.bytes().as_ptr(), address);
                    assert_eq!(
                        texture.read_pixels()?.get_pixel(0, 0),
                        crate::types::Color::rgba(64, 128, 32, 128)
                    );
                    rgba8(
                        &*device.canvas()?,
                        PhysicalRect::from_size(1, 1),
                        &mut source,
                        4,
                    )?;
                    texture.pixels_mut()?.set_pixel(
                        1,
                        1,
                        &crate::types::Color::rgba(200, 100, 50, 128),
                    );
                    Ok(())
                },
            )
            .unwrap();
        });
        assert_eq!(allocations, 0);
        assert_eq!(device.canvas().unwrap().clip_rect(), Some(prior_clip));
        assert_eq!(source, [32, 64, 16, 128]);
        assert_eq!(&bytes[8..16], &[0xa5; 8]);
        assert_eq!(&bytes[24..], &[0xa5; 8]);
        let image = texture(
            &*device.canvas().unwrap(),
            PhysicalRect::from_size(4, 3),
            TargetAlpha::Straight,
        )
        .unwrap();
        let pixels = image.read_pixels().unwrap();
        assert_eq!(
            pixels.get_pixel(2, 2),
            crate::types::Color::rgba(100, 50, 25, 128)
        );
        assert_eq!(
            pixels.get_pixel(0, 0),
            crate::types::Color::rgba(32, 64, 16, 128)
        );

        let error = edit(
            &device,
            region,
            &mut bytes,
            TargetAlpha::Straight,
            &mut |texture| {
                texture.pixels_mut()?.bytes_mut().fill(0);
                Err(RenderError::InvalidGeometry)
            },
        );
        assert_eq!(error, Err(RenderError::InvalidGeometry));
        let after = texture(
            &*device.canvas().unwrap(),
            PhysicalRect::from_size(4, 3),
            TargetAlpha::Straight,
        )
        .unwrap();
        assert_eq!(after.read_pixels().unwrap().bytes(), pixels.bytes());
        assert_eq!(
            edit(
                &device,
                region,
                &mut [],
                TargetAlpha::Straight,
                &mut |_| panic!("invalid storage")
            ),
            Err(RenderError::InvalidTexture)
        );
    }

    #[test]
    fn alpha_conversion_preserves_valid_premultiplied_values_and_row_padding() {
        for alpha in 0..=255u8 {
            for channel in 0..=alpha {
                let mut bytes = [channel, channel, channel, alpha, 0xa5, 0xa5, 0xa5, 0xa5];
                convert_alpha(
                    &mut bytes,
                    1,
                    1,
                    8,
                    TargetAlpha::Premultiplied,
                    TargetAlpha::Straight,
                )
                .unwrap();
                convert_alpha(
                    &mut bytes,
                    1,
                    1,
                    8,
                    TargetAlpha::Straight,
                    TargetAlpha::Premultiplied,
                )
                .unwrap();
                assert_eq!(
                    bytes,
                    [channel, channel, channel, alpha, 0xa5, 0xa5, 0xa5, 0xa5]
                );
            }
        }
        let mut bytes = [0xa5; 8];
        assert_eq!(
            convert_alpha(
                &mut bytes,
                2,
                2,
                8,
                TargetAlpha::Straight,
                TargetAlpha::Premultiplied
            ),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(bytes, [0xa5; 8]);
    }

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
        let snapshot = texture(&canvas, area, TargetAlpha::Straight).unwrap();
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
    fn region_reads_reuse_destination_storage_and_preserve_uncovered_pixels() {
        let surface =
            sdl2::surface::Surface::new(3, 2, sdl2::pixels::PixelFormatEnum::RGBA32).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(32, 64, 16, 128));
        canvas.clear();
        let mut bytes = [0xa5; 44];
        let mut destination = Texture::new(&mut bytes, 3, 3, ColorFormat::RGBA8888)
            .unwrap()
            .with_stride(16)
            .unwrap();
        let allocations = tracked_allocations(|| {
            for _ in 0..3 {
                region_into(
                    &canvas,
                    PhysicalRect::from_size(3, 2),
                    (-1, -1),
                    &mut destination,
                    TargetAlpha::Premultiplied,
                )
                .unwrap();
            }
        });
        assert_eq!(allocations, 0);
        let pixels = destination.read_pixels().unwrap();
        assert_eq!(&pixels.bytes()[..20], &[0xa5; 20]);
        assert_eq!(&pixels.bytes()[20..28], &[64, 128, 32, 128].repeat(2));
        assert_eq!(&pixels.bytes()[28..36], &[0xa5; 8]);
        assert_eq!(&pixels.bytes()[36..44], &[64, 128, 32, 128].repeat(2));
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

    #[test]
    fn source_readback_accepts_omitted_padding_after_the_final_row() {
        let surface =
            sdl2::surface::Surface::new(2, 2, sdl2::pixels::PixelFormatEnum::RGBA32).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(17, 39, 83, 127));
        canvas.clear();
        let mut bytes = [0xa5; 20];
        rgba8(&canvas, PhysicalRect::from_size(2, 2), &mut bytes, 12).unwrap();
        assert_eq!(&bytes[..8], &[17, 39, 83, 127].repeat(2));
        assert_eq!(&bytes[8..12], &[0xa5; 4]);
        assert_eq!(&bytes[12..], &[17, 39, 83, 127].repeat(2));
        rgba8(&canvas, PhysicalRect::from_size(2, 1), &mut bytes[..8], 12).unwrap();
    }
}
