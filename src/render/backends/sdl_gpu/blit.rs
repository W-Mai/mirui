use super::{SdlGpuRenderer, sdl_pixel_rect};
use crate::render::renderer::RenderError;
use crate::render::texture::{ColorFormat, Texture, TexturePixels};
use crate::types::{Point, Rect};

use crate::render::command::CompositeMode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Texture as SdlTexture;

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn texture_format(format: ColorFormat) -> PixelFormatEnum {
        match format {
            ColorFormat::RGBA8888 => PixelFormatEnum::RGBA32,
            ColorFormat::BGRA8888 => PixelFormatEnum::BGRA32,
            ColorFormat::RGB888 => PixelFormatEnum::RGB24,
            ColorFormat::RGB565 | ColorFormat::RGB565Swapped => PixelFormatEnum::RGB565,
        }
    }

    pub(super) fn upload_texture(
        dst: &mut SdlTexture,
        src: &TexturePixels<'_>,
        area: Option<sdl2::rect::Rect>,
    ) -> Result<(), RenderError> {
        crate::trace_span!("sdl.texture.upload");
        let direct = src.direct_format();
        let swap = matches!(
            direct,
            Some(ColorFormat::RGB565 | ColorFormat::RGB565Swapped)
        ) && (direct == Some(ColorFormat::RGB565Swapped)) != cfg!(target_endian = "big");
        let complete_pitch = src
            .stride()
            .checked_mul(usize::from(src.height()))
            .is_some_and(|len| len <= src.bytes().len());
        let result = if direct.is_none() || swap || !complete_pitch {
            dst.with_lock(area, |pixels, pitch| {
                Self::write_rows(src, pixels, pitch, swap)
            })
            .map_err(|_| RenderError::BackendFailure)
            .and_then(core::convert::identity)
        } else {
            dst.update(area, src.bytes(), src.stride())
                .map_err(|_| RenderError::BackendFailure)
        };
        #[cfg(feature = "perf")]
        super::trace_transfer(
            crate::core::perf::TransferKind::SdlTextureUpload,
            src.width(),
            src.height(),
            direct.unwrap_or(ColorFormat::RGBA8888),
            result.is_ok(),
        );
        result
    }

    fn write_rows(
        src: &TexturePixels<'_>,
        dst: &mut [u8],
        pitch: usize,
        swap: bool,
    ) -> Result<(), RenderError> {
        let direct = src.direct_format();
        let row_bytes =
            usize::from(src.width()) * direct.unwrap_or(ColorFormat::RGBA8888).bytes_per_pixel();
        let height = usize::from(src.height());
        if pitch < row_bytes
            || pitch
                .checked_mul(height - 1)
                .and_then(|start| start.checked_add(row_bytes))
                .is_none_or(|required| required > dst.len())
        {
            return Err(RenderError::InvalidTexture);
        }
        if direct.is_none() {
            for y in 0..height {
                let output = &mut dst[y * pitch..][..row_bytes];
                for (out, rgba) in output.chunks_exact_mut(4).zip(src.sampled_rgba_row(y)) {
                    out.copy_from_slice(&rgba);
                }
            }
            return Ok(());
        }
        let source = src.bytes();
        for y in 0..height {
            let input = &source[y * src.stride()..][..row_bytes];
            let output = &mut dst[y * pitch..][..row_bytes];
            if swap {
                for (input, output) in input.chunks_exact(2).zip(output.chunks_exact_mut(2)) {
                    output.copy_from_slice(&[input[1], input[0]]);
                }
            } else {
                output.copy_from_slice(input);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn blit_inner(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
        composite: CompositeMode,
        source: Option<&crate::render::texture::TextureRegion<'_>>,
    ) {
        if opa == 0 {
            return;
        }
        // SDL2's exposed BlendMode set covers SourceOver / Add / Multiply
        // bit-identically; the other 4 modes need SDL_ComposeCustomBlend
        // factor combinations the safe `sdl2` crate doesn't expose,
        // so fall back to a panic that points users at SwRenderer.
        let sdl_blend = match composite {
            CompositeMode::SourceOver => sdl2::render::BlendMode::Blend,
            CompositeMode::Add => sdl2::render::BlendMode::Add,
            CompositeMode::Multiply => sdl2::render::BlendMode::Mul,
            CompositeMode::Screen
            | CompositeMode::Darken
            | CompositeMode::Lighten
            | CompositeMode::Difference => unimplemented!(
                "sdl_gpu backend: composite {composite:?} requires SDL_ComposeCustomBlendMode; use SwRenderer"
            ),
        };

        let phys_dst = self.viewport.point_to_physical(dst);
        let phys_dst_size = self.viewport.point_to_physical(dst_size);
        let phys_clip = self.physical_draw_clip(*clip);
        let dx = phys_dst.x.to_int();
        let dy = phys_dst.y.to_int();
        let dw = phys_dst_size.x.to_int().max(0) as u32;
        let dh = phys_dst_size.y.to_int().max(0) as u32;
        if dw == 0 || dh == 0 {
            return;
        }
        let (sx0, sy0, sx1, sy1) = src_rect.pixel_bounds();
        let (Ok(sw), Ok(sh)) = (u32::try_from(sx1 - sx0), u32::try_from(sy1 - sy0)) else {
            return;
        };
        let src_sdl = sdl2::rect::Rect::new(sx0.max(0), sy0.max(0), sw, sh);
        let dst_sdl = sdl2::rect::Rect::new(dx, dy, dw, dh);

        let Some(sdl_clip) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
            return;
        };

        let uploaded = self
            .texture_cache
            .with_texture(src, composite, source, |tex, alpha| {
                let mut canvas = self.device.canvas()?;
                if alpha == crate::render::TargetAlpha::Straight {
                    tex.set_blend_mode(sdl_blend);
                }
                tex.set_alpha_mod(opa);
                let rgb = if alpha == crate::render::TargetAlpha::Premultiplied {
                    opa
                } else {
                    255
                };
                tex.set_color_mod(rgb, rgb, rgb);

                canvas.set_clip_rect(sdl_clip);
                let copied = canvas.copy(tex, Some(src_sdl), Some(dst_sdl)).is_ok();
                self.device.finish_draw(&mut canvas);
                copied.then_some(()).ok_or(RenderError::BackendFailure)
            });
        if let Err(error) = uploaded.and_then(core::convert::identity) {
            self.failure.get_or_insert(error);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/blit.rs"]
mod tests;
