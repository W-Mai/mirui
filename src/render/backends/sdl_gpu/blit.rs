use super::{SdlGpuRenderer, sdl_pixel_rect};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Point, Rect};

impl SdlGpuRenderer<'_> {
    pub(super) fn blit_inner(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        _opa: u8,
    ) {
        let sdl_fmt = match src.format {
            ColorFormat::RGBA8888 => sdl2::pixels::PixelFormatEnum::RGBA32,
            ColorFormat::BGRA8888 => sdl2::pixels::PixelFormatEnum::BGRA32,
            ColorFormat::RGB888 => sdl2::pixels::PixelFormatEnum::RGB24,
            ColorFormat::RGB565 => sdl2::pixels::PixelFormatEnum::RGB565,
            ColorFormat::RGB565Swapped => {
                // SDL has no BGR565 variant; a RGB565 round-trip would
                // swap channels. Punt until we need it in practice.
                return;
            }
        };

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

        let src_slice = src.buf.as_slice();
        let stride = src.stride;
        let src_width = src.width as u32;
        let src_height = src.height as u32;

        let canvas = &mut *self.canvas;
        self.label_cache.with_creator(|creator| {
            let mut tex = match creator.create_texture_streaming(sdl_fmt, src_width, src_height) {
                Ok(t) => t,
                Err(_) => return,
            };
            if tex.update(None, src_slice, stride).is_err() {
                return;
            }
            tex.set_blend_mode(sdl2::render::BlendMode::Blend);

            canvas.set_clip_rect(sdl_clip);
            let _ = canvas.copy(&tex, Some(src_sdl), Some(dst_sdl));
            canvas.set_clip_rect(None);
        });
    }
}
