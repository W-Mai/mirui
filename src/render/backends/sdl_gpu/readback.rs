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
    crate::trace_span!("sdl.target.edit");
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
    crate::trace_span!("sdl.target.edit.body", { body(&mut texture) })?;
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
    let update = crate::trace_span!("sdl.target.writeback", {
        uploaded
            .update(None, pixels.bytes(), stride)
            .map_err(|_| RenderError::BackendFailure)
    });
    #[cfg(feature = "perf")]
    super::trace_transfer(
        crate::core::perf::TransferKind::SdlTargetWriteback,
        area.width(),
        area.height(),
        ColorFormat::RGBA8888,
        update.is_ok(),
    );
    update?;
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
    crate::trace_span!("sdl.target.readback");
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
    let result = if result == 0 {
        Ok(())
    } else {
        Err(RenderError::BackendFailure)
    };
    #[cfg(feature = "perf")]
    if let (Ok(width), Ok(height)) = (u16::try_from(rect.w), u16::try_from(rect.h)) {
        super::trace_transfer(
            crate::core::perf::TransferKind::SdlTargetReadback,
            width,
            height,
            ColorFormat::RGBA8888,
            result.is_ok(),
        );
    }
    result
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/readback.rs"]
mod tests;
