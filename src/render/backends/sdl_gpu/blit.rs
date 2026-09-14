use super::{SdlGpuRenderer, sdl_pixel_rect};
use crate::render::texture::{ColorFormat, Texture};
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

    pub(super) fn upload_texture(dst: &mut SdlTexture, src: &Texture) -> bool {
        if !src.valid_storage() {
            return false;
        }
        let swap = matches!(src.format, ColorFormat::RGB565 | ColorFormat::RGB565Swapped)
            && (src.format == ColorFormat::RGB565Swapped) != cfg!(target_endian = "big");
        let complete_pitch = src
            .stride
            .checked_mul(usize::from(src.height))
            .is_some_and(|len| len <= src.buf.as_slice().len());
        if swap || !complete_pitch {
            dst.with_lock(None, |pixels, pitch| {
                Self::write_rows(src, pixels, pitch, swap)
            })
            .unwrap_or(false)
        } else {
            dst.update(None, src.buf.as_slice(), src.stride).is_ok()
        }
    }

    fn write_rows(src: &Texture, dst: &mut [u8], pitch: usize, swap: bool) -> bool {
        if !src.valid_storage() {
            return false;
        }
        let row_bytes = usize::from(src.width) * src.format.bytes_per_pixel();
        let height = usize::from(src.height);
        if pitch < row_bytes
            || pitch
                .checked_mul(height - 1)
                .and_then(|start| start.checked_add(row_bytes))
                .is_none_or(|required| required > dst.len())
        {
            return false;
        }
        let source = src.buf.as_slice();
        for y in 0..height {
            let input = &source[y * src.stride..][..row_bytes];
            let output = &mut dst[y * pitch..][..row_bytes];
            if swap {
                for (input, output) in input.chunks_exact(2).zip(output.chunks_exact_mut(2)) {
                    output.copy_from_slice(&[input[1], input[0]]);
                }
            } else {
                output.copy_from_slice(input);
            }
        }
        true
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
        let sdl_fmt = Self::texture_format(src.format);

        let phys_dst = self.viewport.point_to_physical(dst);
        let phys_dst_size = self.viewport.point_to_physical(dst_size);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let dx = phys_dst.x.to_int();
        let dy = phys_dst.y.to_int();
        let dw = phys_dst_size.x.to_int().max(0) as u32;
        let dh = phys_dst_size.y.to_int().max(0) as u32;
        if dw == 0 || dh == 0 {
            return;
        }
        let (sx0, sy0, sx1, sy1) = src_rect.pixel_bounds();
        let src_sdl = sdl2::rect::Rect::new(
            sx0.max(0),
            sy0.max(0),
            (sx1 - sx0) as u32,
            (sy1 - sy0) as u32,
        );
        let dst_sdl = sdl2::rect::Rect::new(dx, dy, dw, dh);

        let Some(sdl_clip) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
            return;
        };

        let src_width = src.width as u32;
        let src_height = src.height as u32;

        let canvas = &mut *self.canvas;
        let mut uploaded = false;
        self.label_cache.with_creator(|creator| {
            let mut tex = match creator.create_texture_streaming(sdl_fmt, src_width, src_height) {
                Ok(t) => t,
                Err(_) => return,
            };
            if !Self::upload_texture(&mut tex, src) {
                return;
            }
            tex.set_blend_mode(sdl_blend);
            // SDL2 modulates texture sample alpha by alpha_mod/255 in the
            // copy() path, so set_alpha_mod composes with src.a as the
            // group opacity multiplier.
            tex.set_alpha_mod(opa);

            canvas.set_clip_rect(sdl_clip);
            uploaded = canvas.copy(&tex, Some(src_sdl), Some(dst_sdl)).is_ok();
            canvas.set_clip_rect(None);
        });
        if !uploaded {
            self.draw_failed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_upload_skips_source_and_destination_padding() {
        let bytes = [
            0xf8, 0x00, 0x07, 0xe0, 0x00, 0x1f, 0xaa, 0xbb, 0xff, 0xe0, 0x07, 0xff, 0xf8, 0x1f,
        ];
        let mut src = Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565Swapped);
        src.stride = 8;
        let mut dst = [0xcc; 20];
        assert!(SdlGpuRenderer::<Box<[u8]>>::write_rows(
            &src, &mut dst, 10, true
        ));
        assert_eq!(
            &dst[..10],
            &[0x00, 0xf8, 0xe0, 0x07, 0x1f, 0x00, 0xcc, 0xcc, 0xcc, 0xcc]
        );
        assert_eq!(
            &dst[10..],
            &[0xe0, 0xff, 0xff, 0x07, 0x1f, 0xf8, 0xcc, 0xcc, 0xcc, 0xcc]
        );
        assert!(!SdlGpuRenderer::<Box<[u8]>>::write_rows(
            &src,
            &mut dst[..15],
            10,
            true
        ));
        assert!(SdlGpuRenderer::<Box<[u8]>>::write_rows(
            &src, &mut dst, 10, false
        ));
        assert_eq!(&dst[..6], &bytes[..6]);
        assert_eq!(&dst[10..16], &bytes[8..14]);
    }
}
