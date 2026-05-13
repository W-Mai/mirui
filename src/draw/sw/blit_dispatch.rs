use super::SwRenderer;
use super::blit_fast::{blit_1to1_fast, blit_2to2_fast, blit_dda};
use crate::draw::texture::Texture;
use crate::types::{Point, Rect};

impl SwRenderer<'_> {
    pub(super) fn blit_inner(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
    ) {
        let phys_dst = self.viewport.point_to_physical(dst);
        let phys_dst_size = self.viewport.point_to_physical(dst_size);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let (sx0, sy0, sw, sh) = src_rect.to_px();
        let (clip_x0, clip_y0, clip_x1, clip_y1) = phys_clip.pixel_bounds();
        let dx0 = phys_dst.x.to_int();
        let dy0 = phys_dst.y.to_int();
        let dw = phys_dst_size.x.to_int();
        let dh = phys_dst_size.y.to_int();
        if dw <= 0 || dh <= 0 || sw == 0 || sh == 0 {
            return;
        }

        let sw_i = sw as i32;
        let sh_i = sh as i32;
        // 1× → format-specialized 1to1 fast path; 2× → 2to2; arbitrary → DDA.
        #[allow(clippy::if_same_then_else)]
        if dw == sw_i && dh == sh_i {
            blit_1to1_fast(
                &mut self.target,
                src,
                sx0,
                sy0,
                sw,
                sh,
                dx0,
                dy0,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
            );
        } else if dw == sw_i * 2 && dh == sh_i * 2 {
            blit_2to2_fast(
                &mut self.target,
                src,
                sx0,
                sy0,
                sw,
                sh,
                dx0,
                dy0,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
            );
        } else {
            blit_dda(
                &mut self.target,
                src,
                sx0,
                sy0,
                sw,
                sh,
                dx0,
                dy0,
                dw,
                dh,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
            );
        }
    }
}
