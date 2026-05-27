use crate::types::{Color, Fixed, Point, Rect, Transform, Viewport};

use super::canvas::Canvas;
use super::command::DrawCommand;
use super::path::Path;
use super::renderer::Renderer;
use super::texture::Texture;

#[cfg(feature = "perf")]
pub mod perf;
#[cfg(feature = "perf")]
pub use perf::{PerfCtx, quad_perf};

mod blit_dispatch;
mod blit_fast;
pub mod blur;
mod label;
pub mod mix;
mod path;
mod quad;
mod quad_aa;
mod rect_fill;
mod rect_stroke;
mod transformed;
use quad::{blit_quad, fill_rect_quad, stroke_rect_quad};
use transformed::{blit_transformed, fill_rect_transformed, offset_point, offset_rect};

pub struct SwRenderer<'a> {
    pub target: Texture<'a>,
    pub viewport: Viewport,
    #[cfg(feature = "perf")]
    pub perf: Option<PerfCtx>,
}

impl<'a> SwRenderer<'a> {
    pub fn new(target: Texture<'a>) -> Self {
        let w = target.width;
        let h = target.height;
        Self {
            target,
            viewport: Viewport::new(w, h, Fixed::ONE),
            #[cfg(feature = "perf")]
            perf: None,
        }
    }
}

impl<'a> SwRenderer<'a> {
    fn draw_transformed(&mut self, cmd: &DrawCommand, clip: &Rect, tf: &Transform) {
        let vp = self.viewport.as_transform();
        let phys_tf = vp.compose(tf);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        match cmd {
            DrawCommand::Fill {
                area, color, opa, ..
            } => {
                fill_rect_transformed(&mut self.target, *area, phys_clip, &phys_tf, color, *opa);
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let dst = Rect {
                    x: pos.x,
                    y: pos.y,
                    w: size.x,
                    h: size.y,
                };
                blit_transformed(
                    &mut self.target,
                    texture,
                    &src_rect,
                    dst,
                    phys_clip,
                    &phys_tf,
                );
            }
            _ => unimplemented!(
                "sw backend: {:?} under non-axis-aligned transform not yet supported",
                core::mem::discriminant(cmd)
            ),
        }
    }
}

impl<'a> Canvas for SwRenderer<'a> {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.fill_path_inner(path, clip, color, opa);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.stroke_path_inner(path, clip, width, color, opa);
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, clip, color, radius, opa);
    }

    fn stroke_rect(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        self.stroke_rect_inner(area, clip, width, color, radius, opa);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        self.blit_inner(src, src_rect, dst, dst_size, clip);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = phys_area.intersect(&screen) else {
            return;
        };
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                self.target.set_pixel(px, py, color);
            }
        }
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.draw_label_inner(pos, text, clip, color, opa);
    }

    fn flush(&mut self) {}
}

impl SwRenderer<'_> {
    fn dispatch_fill_quad(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = quad_perf::now();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let s = self.viewport.scale();
        fill_rect_quad(
            &mut self.target,
            &phys_q,
            phys_clip,
            color,
            radius * s,
            area.w * s,
            area.h * s,
            opa,
        );
        #[cfg(feature = "perf")]
        quad_perf::add_fill(quad_perf::now().wrapping_sub(t0));
    }

    fn dispatch_blit_quad(&mut self, q: &[Point; 4], texture: &Texture, clip: &Rect) {
        #[cfg(feature = "perf")]
        let t0 = quad_perf::now();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        blit_quad(&mut self.target, texture, &phys_q, phys_clip);
        #[cfg(feature = "perf")]
        quad_perf::add_blit(quad_perf::now().wrapping_sub(t0));
    }

    fn dispatch_border_quad(
        &mut self,
        q: &[Point; 4],
        color: &Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
        clip: &Rect,
    ) {
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let s = self.viewport.scale();
        stroke_rect_quad(
            &mut self.target,
            &phys_q,
            phys_clip,
            color,
            width * s,
            radius * s,
            opa,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_fill(
        &mut self,
        area: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let area = offset_rect(area, tx, ty);
        self.fill_rect(&area, clip, color, radius, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.fill += (p.clock)() - t0;
            p.count_fill += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_border(
        &mut self,
        area: &Rect,
        color: &Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let area = offset_rect(area, tx, ty);
        self.stroke_rect(&area, clip, width, color, radius, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }

    fn dispatch_blit(
        &mut self,
        pos: &Point,
        size: Point,
        texture: &Texture,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let src_rect = Rect::new(0, 0, texture.width, texture.height);
        let pos = offset_point(pos, tx, ty);
        self.blit(texture, &src_rect, pos, size, clip);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.blit += (p.clock)() - t0;
            p.count_blit += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_label(
        &mut self,
        pos: &Point,
        text: &[u8],
        color: &Color,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let pos = offset_point(pos, tx, ty);
        self.draw_label(&pos, text, clip, color, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.label += (p.clock)() - t0;
            p.count_label += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_line(
        &mut self,
        p1: &Point,
        p2: &Point,
        color: &Color,
        width: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let p1 = offset_point(p1, tx, ty);
        let p2 = offset_point(p2, tx, ty);
        self.draw_line(p1, p2, clip, width, color, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_arc(
        &mut self,
        center: &Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: &Color,
        width: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let center = offset_point(center, tx, ty);
        self.draw_arc(
            center,
            radius,
            start_angle,
            end_angle,
            clip,
            width,
            color,
            opa,
        );
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }
}

impl Renderer for SwRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Quad fast paths short-circuit before the translate/transform branch.
        if let DrawCommand::Fill {
            area,
            quad: Some(q),
            color,
            opa,
            radius,
            ..
        } = cmd
        {
            crate::trace_span!("sw.fill_quad");
            self.dispatch_fill_quad(q, area, color, *radius, *opa, clip);
            return;
        }
        if let DrawCommand::Blit {
            quad: Some(q),
            texture,
            ..
        } = cmd
        {
            crate::trace_span!("sw.blit_quad");
            self.dispatch_blit_quad(q, texture, clip);
            return;
        }
        if let DrawCommand::Border {
            quad: Some(q),
            color,
            width,
            radius,
            opa,
            ..
        } = cmd
        {
            crate::trace_span!("sw.border_quad");
            self.dispatch_border_quad(q, color, *width, *radius, *opa, clip);
            return;
        }

        let tf = cmd.transform();
        let class = tf.classify();
        if !matches!(class, TransformClass::Identity | TransformClass::Translate) {
            crate::trace_span!("sw.transformed");
            self.draw_transformed(cmd, clip, &tf);
            return;
        }
        let (tx, ty) = match class {
            TransformClass::Identity => (Fixed::ZERO, Fixed::ZERO),
            TransformClass::Translate => (tf.tx, tf.ty),
            _ => unreachable!(),
        };
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                crate::trace_span!("sw.fill");
                self.dispatch_fill(area, color, *radius, *opa, tx, ty, clip);
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                crate::trace_span!("sw.border");
                self.dispatch_border(area, color, *width, *radius, *opa, tx, ty, clip);
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                crate::trace_span!("sw.blit");
                self.dispatch_blit(pos, *size, texture, tx, ty, clip);
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                crate::trace_span!("sw.label");
                self.dispatch_label(pos, text, color, *opa, tx, ty, clip);
            }
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
                ..
            } => {
                crate::trace_span!("sw.line");
                self.dispatch_line(p1, p2, color, *width, *opa, tx, ty, clip);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
                ..
            } => {
                crate::trace_span!("sw.arc");
                self.dispatch_arc(
                    center,
                    *radius,
                    *start_angle,
                    *end_angle,
                    color,
                    *width,
                    *opa,
                    tx,
                    ty,
                    clip,
                );
            }
            DrawCommand::FillPath {
                path, color, opa, ..
            } => {
                crate::trace_span!("sw.fill_path");
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, color, *opa);
                } else {
                    let translated = translate_path(path, tx, ty);
                    self.fill_path_inner(&translated, clip, color, *opa);
                }
            }
        }
    }

    fn flush(&mut self) {
        Canvas::flush(self);
    }

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::draw::texture::ColorFormat> {
        Some(self.target.format)
    }

    fn sample_target_region(&self, src: &Rect) -> Option<crate::draw::texture::Texture<'static>> {
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        let w = (sx1 - sx0).max(1) as u16;
        let h = (sy1 - sy0).max(1) as u16;
        let mut tex = crate::draw::texture::Texture::owned(w, h, self.target.format);
        self.read_target_region(src, &mut tex);
        Some(tex)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::draw::texture::Texture),
    ) -> bool {
        use crate::draw::texture::{TexBuf, Texture};
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        let target_w = self.target.width as i32;
        let target_h = self.target.height as i32;
        let cx0 = sx0.max(0);
        let cy0 = sy0.max(0);
        let cx1 = sx1.min(target_w);
        let cy1 = sy1.min(target_h);
        if cx1 <= cx0 || cy1 <= cy0 {
            return false;
        }
        let bpp = self.target.format.bytes_per_pixel();
        let target_stride = self.target.stride;
        let off_start = cy0 as usize * target_stride + cx0 as usize * bpp;
        // `(row_h - 1) * stride + row_w * bpp` so the slice ends at
        // the rect's last pixel, not the last full stride row — the
        // gap past `width` per row is shared with neighbouring
        // entities and must stay outside our borrow.
        let row_w = (cx1 - cx0) as u16;
        let row_h = (cy1 - cy0) as u16;
        let view_bytes = (row_h as usize - 1) * target_stride + row_w as usize * bpp;
        let buf = &mut self.target.buf.as_mut_slice()[off_start..off_start + view_bytes];
        let mut view = Texture {
            buf: TexBuf::Mut(buf),
            width: row_w,
            height: row_h,
            format: self.target.format,
            stride: target_stride,
        };
        f(&mut view);
        true
    }

    fn read_target_region(&self, src: &Rect, dst: &mut crate::draw::texture::Texture) {
        // Caller may pass a logical-sized dst; clipping to the
        // overlap avoids stretched top-left samples on HiDPI.
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        let target_w = self.target.width as i32;
        let target_h = self.target.height as i32;
        let copy_w = ((sx1 - sx0).min(dst.width as i32)).max(0);
        let copy_h = ((sy1 - sy0).min(dst.height as i32)).max(0);
        if copy_w == 0 || copy_h == 0 {
            return;
        }

        // Same-format fast path: row-wise byte memcpy. Both callers
        // (`try_draw_offscreen` pre-seed, `sample_target_region`)
        // allocate dst at the target's format, so this normally hits.
        if self.target.format == dst.format {
            let bpp = dst.format.bytes_per_pixel();
            let target_buf = self.target.buf.as_slice();
            let dst_buf = dst.buf.as_mut_slice();
            let target_stride = self.target.stride;
            let dst_stride = dst.stride;
            for dy in 0..copy_h as usize {
                let phys_y = sy0 + dy as i32;
                if phys_y < 0 || phys_y >= target_h {
                    continue;
                }
                let src_x_start = sx0.max(0);
                let src_x_end = (sx0 + copy_w).min(target_w);
                if src_x_end <= src_x_start {
                    continue;
                }
                let dst_x_start = (src_x_start - sx0) as usize;
                let row_bytes = (src_x_end - src_x_start) as usize * bpp;
                let src_row_off = phys_y as usize * target_stride + src_x_start as usize * bpp;
                let dst_row_off = dy * dst_stride + dst_x_start * bpp;
                dst_buf[dst_row_off..dst_row_off + row_bytes]
                    .copy_from_slice(&target_buf[src_row_off..src_row_off + row_bytes]);
            }
            return;
        }

        for dy in 0..copy_h {
            for dx in 0..copy_w {
                let phys_x = sx0 + dx;
                let phys_y = sy0 + dy;
                if phys_x < 0 || phys_y < 0 || phys_x >= target_w || phys_y >= target_h {
                    continue;
                }
                let px = self.target.get_pixel(phys_x, phys_y);
                dst.set_pixel(dx, dy, &px);
            }
        }
    }
}

fn translate_path(path: &Path, tx: Fixed, ty: Fixed) -> Path {
    use super::path::PathCmd;
    let cmds = path
        .cmds
        .iter()
        .map(|c| match c {
            PathCmd::MoveTo(p) => PathCmd::MoveTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::LineTo(p) => PathCmd::LineTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                ctrl: Point {
                    x: ctrl.x + tx,
                    y: ctrl.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
                ctrl1: Point {
                    x: ctrl1.x + tx,
                    y: ctrl1.y + ty,
                },
                ctrl2: Point {
                    x: ctrl2.x + tx,
                    y: ctrl2.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::Close => PathCmd::Close,
        })
        .collect();
    Path { cmds }
}

#[cfg(test)]
mod tests {
    use super::blit_fast::{blit_1to1_fast, blit_2to2_fast, blit_dda, blit_generic_slow};
    use super::*;
    use crate::draw::texture::ColorFormat;
    use alloc::vec;

    /// Blit dst origin at a negative x — common when an OffscreenRender
    /// entity's WidgetTransform translates the buffer off the left
    /// edge — must not write to wrong target rows. Pre-fix, blit_inner
    /// passed `dx0=-N` straight to the path that did `dx0 as usize * 4`
    /// for the row offset, wrapping into a far positive byte index and
    /// scribbling the source texture into a different row of the
    /// target.
    #[test]
    fn blit_at_negative_x_does_not_wrap_into_wrong_rows() {
        let mut tgt_buf = vec![0u8; 64 * 64 * 4];
        let tgt_tex = Texture::new(&mut tgt_buf, 64, 64, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tgt_tex);

        let mut src_buf = vec![0u8; 32 * 32 * 4];
        for px in src_buf.chunks_exact_mut(4) {
            px[0] = 200;
            px[1] = 100;
            px[2] = 50;
            px[3] = 255;
        }
        let src_tex = Texture::new(&mut src_buf, 32, 32, ColorFormat::RGBA8888);
        let src_rect = Rect::new(0, 0, 32, 32);

        let dst = Point::new(Fixed::from_int(-24), Fixed::from_int(8));
        let dst_size = Point::new(Fixed::from_int(32), Fixed::from_int(32));
        let clip = Rect {
            x: Fixed::from_int(-24),
            y: Fixed::from_int(0),
            w: Fixed::from_int(80),
            h: Fixed::from_int(64),
        };
        backend.blit(&src_tex, &src_rect, dst, dst_size, &clip);

        for y in 8..40 {
            for x in 0..8 {
                let p = backend.target.get_pixel(x, y);
                assert_eq!(
                    (p.r, p.g, p.b),
                    (200, 100, 50),
                    "visible pixel ({},{}) should be source colour",
                    x,
                    y
                );
            }
        }
        // Anywhere outside the visible band must stay zeroed; the
        // pre-fix bug wrote source colour into row 8 columns ~40-64
        // because dx0=-24 cast to usize wrapped the row offset.
        for y in 0..64 {
            for x in 0..64 {
                let in_visible = (8..40).contains(&y) && x < 8;
                if in_visible {
                    continue;
                }
                let p = backend.target.get_pixel(x, y);
                assert_eq!(
                    (p.r, p.g, p.b, p.a),
                    (0, 0, 0, 0),
                    "out-of-visible pixel ({},{}) was written",
                    x,
                    y
                );
            }
        }
    }

    /// Same negative-x guard as the RGBA8888 case but exercising the
    /// RGB565Swapped → RGB565Swapped fast path, which has its own
    /// `dx0 as usize * 2` byte-offset arithmetic.
    #[test]
    fn blit_at_negative_x_rgb565_swapped_does_not_wrap() {
        let mut tgt_buf = vec![0u8; 64 * 64 * 2];
        let tgt_tex = Texture::new(&mut tgt_buf, 64, 64, ColorFormat::RGB565Swapped);
        let mut backend = SwRenderer::new(tgt_tex);

        let mut src_buf = vec![0u8; 32 * 32 * 2];
        for px in src_buf.chunks_exact_mut(2) {
            // RGB565Swapped: hi byte first. Pack red = 0xF800,
            // wire bytes [0xF8, 0x00].
            px[0] = 0xF8;
            px[1] = 0x00;
        }
        let src_tex = Texture::new(&mut src_buf, 32, 32, ColorFormat::RGB565Swapped);
        let src_rect = Rect::new(0, 0, 32, 32);

        let dst = Point::new(Fixed::from_int(-24), Fixed::from_int(8));
        let dst_size = Point::new(Fixed::from_int(32), Fixed::from_int(32));
        let clip = Rect {
            x: Fixed::from_int(-24),
            y: Fixed::from_int(0),
            w: Fixed::from_int(80),
            h: Fixed::from_int(64),
        };
        backend.blit(&src_tex, &src_rect, dst, dst_size, &clip);

        for y in 8..40 {
            for x in 0..8 {
                let p = backend.target.get_pixel(x, y);
                assert_eq!(p.r, 248, "visible pixel ({},{}) should be red", x, y);
            }
        }
        for y in 0..64 {
            for x in 0..64 {
                let in_visible = (8..40).contains(&y) && x < 8;
                if in_visible {
                    continue;
                }
                let off = (y as usize * 64 + x as usize) * 2;
                assert_eq!(
                    (
                        backend.target.buf.as_slice()[off],
                        backend.target.buf.as_slice()[off + 1]
                    ),
                    (0, 0),
                    "out-of-visible pixel ({},{}) was written",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn fill_rect_basic() {
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let rect = Rect::new(2, 2, 4, 4);
        let clip = Rect::new(0, 0, 16, 16);
        backend.fill_rect(&rect, &clip, &Color::rgb(255, 0, 0), Fixed::ZERO, 255);

        let c = backend.target.get_pixel(3, 3);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        let c = backend.target.get_pixel(0, 0);
        assert_eq!(c.r, 0);
    }

    #[test]
    fn clear_fills_area() {
        let mut buf = vec![0u8; 8 * 8 * 4];
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        backend.clear(&Rect::new(0, 0, 8, 8), &Color::rgb(50, 100, 150));

        let c = backend.target.get_pixel(4, 4);
        assert_eq!(c.r, 50);
        assert_eq!(c.g, 100);
        assert_eq!(c.b, 150);
    }

    #[test]
    fn fill_path_rect_matches_fill_rect() {
        // A rectangular Path should produce the same interior pixels as fill_rect.
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let path = super::super::path::Path::rect(
            Fixed::from_int(2),
            Fixed::from_int(2),
            Fixed::from_int(8),
            Fixed::from_int(8),
        );
        let clip = Rect::new(0, 0, 16, 16);
        backend.fill_path(&path, &clip, &Color::rgb(0, 0, 255), 255);

        let c = backend.target.get_pixel(5, 5);
        assert_eq!(c.b, 255);
        assert_eq!(c.r, 0);
        let c = backend.target.get_pixel(0, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn fill_path_empty_is_noop() {
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let path = super::super::path::Path::new();
        let clip = Rect::new(0, 0, 4, 4);
        backend.fill_path(&path, &clip, &Color::rgb(255, 255, 255), 255);

        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(backend.target.get_pixel(x, y).r, 0);
            }
        }
    }

    #[test]
    fn fill_path_zero_opa_is_noop() {
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let path = super::super::path::Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(4),
            Fixed::from_int(4),
        );
        let clip = Rect::new(0, 0, 4, 4);
        backend.fill_path(&path, &clip, &Color::rgb(255, 0, 0), 0);

        assert_eq!(backend.target.get_pixel(2, 2).r, 0);
    }

    #[test]
    fn fill_path_triangle_interior_vs_exterior() {
        // Right triangle with vertices (0,0), (10,0), (0,10). Probe interior
        // point (2,2) vs exterior point (8,8).
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let mut path = super::super::path::Path::new();
        path.move_to(Point {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        })
        .line_to(Point {
            x: Fixed::from_int(10),
            y: Fixed::ZERO,
        })
        .line_to(Point {
            x: Fixed::ZERO,
            y: Fixed::from_int(10),
        })
        .close();

        let clip = Rect::new(0, 0, 16, 16);
        backend.fill_path(&path, &clip, &Color::rgb(0, 200, 0), 255);

        assert_eq!(backend.target.get_pixel(2, 2).g, 200);
        assert_eq!(backend.target.get_pixel(8, 8).g, 0);
    }

    #[test]
    fn draw_label_is_reachable_via_trait() {
        // Exercises the trait dispatch path rather than the glyph pixels —
        // just verifies the method exists on Canvas and writes something.
        let mut buf = vec![0u8; 32 * 16 * 4];
        let tex = Texture::new(&mut buf, 32, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let pos = Point {
            x: Fixed::from_int(1),
            y: Fixed::from_int(1),
        };
        let clip = Rect::new(0, 0, 32, 16);
        Canvas::draw_label(&mut backend, &pos, b"A", &clip, &Color::rgb(255, 0, 0), 255);

        let mut found = false;
        for y in 0..16 {
            for x in 0..32 {
                if backend.target.get_pixel(x, y).r > 0 {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "expected at least one red pixel from glyph");
    }

    #[test]
    fn stroke_path_line_colors_interior_and_skips_far_pixels() {
        // Horizontal line from (2,8) to (14,8), width=2. Interior pixels
        // around y=8 should be colored; pixels several rows away must not.
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let mut path = super::super::path::Path::new();
        path.move_to(Point {
            x: Fixed::from_int(2),
            y: Fixed::from_int(8),
        })
        .line_to(Point {
            x: Fixed::from_int(14),
            y: Fixed::from_int(8),
        });

        let clip = Rect::new(0, 0, 16, 16);
        backend.stroke_path(
            &path,
            &clip,
            Fixed::from_int(2),
            &Color::rgb(255, 0, 0),
            255,
        );

        assert!(backend.target.get_pixel(8, 8).r > 0);
        assert_eq!(backend.target.get_pixel(8, 0).r, 0);
        assert_eq!(backend.target.get_pixel(8, 15).r, 0);
    }

    #[test]
    fn renderer_dispatches_line_command() {
        use crate::draw::renderer::Renderer;
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let cmd = DrawCommand::Line {
            p1: Point {
                x: Fixed::from_int(2),
                y: Fixed::from_int(8),
            },
            p2: Point {
                x: Fixed::from_int(14),
                y: Fixed::from_int(8),
            },
            transform: crate::types::Transform::IDENTITY,
            color: Color::rgb(255, 0, 0),
            width: Fixed::from_int(2),
            opa: 255,
        };
        let clip = Rect::new(0, 0, 16, 16);
        Renderer::draw(&mut backend, &cmd, &clip);

        assert!(backend.target.get_pixel(8, 8).r > 0);
    }

    #[test]
    fn renderer_dispatches_arc_command() {
        use crate::draw::renderer::Renderer;
        let mut buf = vec![0u8; 32 * 32 * 4];
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let cmd = DrawCommand::Arc {
            center: Point {
                x: Fixed::from_int(16),
                y: Fixed::from_int(16),
            },
            transform: crate::types::Transform::IDENTITY,
            radius: Fixed::from_int(10),
            start_angle: Fixed::from_int(0),
            end_angle: Fixed::from_int(90),
            color: Color::rgb(0, 255, 0),
            width: Fixed::from_int(2),
            opa: 255,
        };
        let clip = Rect::new(0, 0, 32, 32);
        Renderer::draw(&mut backend, &cmd, &clip);

        let hit = backend.target.get_pixel(26, 16).g > 0 || backend.target.get_pixel(25, 16).g > 0;
        assert!(hit);
    }

    #[test]
    fn draw_line_default_impl_strokes_pixels() {
        // Exercises Canvas::draw_line's default trait impl → stroke_path.
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let p1 = Point {
            x: Fixed::from_int(2),
            y: Fixed::from_int(8),
        };
        let p2 = Point {
            x: Fixed::from_int(14),
            y: Fixed::from_int(8),
        };
        let clip = Rect::new(0, 0, 16, 16);
        backend.draw_line(
            p1,
            p2,
            &clip,
            Fixed::from_int(2),
            &Color::rgb(255, 0, 0),
            255,
        );

        assert!(backend.target.get_pixel(8, 8).r > 0);
    }

    #[test]
    fn draw_arc_default_impl_strokes_pixels() {
        let mut buf = vec![0u8; 32 * 32 * 4];
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let center = Point {
            x: Fixed::from_int(16),
            y: Fixed::from_int(16),
        };
        let clip = Rect::new(0, 0, 32, 32);
        backend.draw_arc(
            center,
            Fixed::from_int(10),
            Fixed::from_int(0),
            Fixed::from_int(90),
            &clip,
            Fixed::from_int(2),
            &Color::rgb(0, 255, 0),
            255,
        );

        // The 0°→90° arc runs from (+radius, 0) to (0, +radius) relative to
        // center. Sample a point on the arc and verify green is present.
        assert!(backend.target.get_pixel(26, 16).g > 0 || backend.target.get_pixel(25, 16).g > 0);
    }

    #[test]
    fn stroke_path_zero_width_is_noop() {
        let mut buf = vec![0u8; 8 * 8 * 4];
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let mut path = super::super::path::Path::new();
        path.move_to(Point {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        })
        .line_to(Point {
            x: Fixed::from_int(8),
            y: Fixed::ZERO,
        });

        let clip = Rect::new(0, 0, 8, 8);
        backend.stroke_path(&path, &clip, Fixed::ZERO, &Color::rgb(255, 0, 0), 255);

        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(backend.target.get_pixel(x, y).r, 0);
            }
        }
    }

    #[test]
    fn painter_fill_rect_with_backend() {
        use crate::draw::painter::Painter;

        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        {
            let mut painter = Painter::new(&mut backend);
            let rect = Rect::new(1, 1, 6, 6);
            let clip = Rect::new(0, 0, 16, 16);
            painter.fill_rect(&rect, &clip, &Color::rgb(0, 255, 0), Fixed::ZERO, 255);
        }

        let c = backend.target.get_pixel(3, 3);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn painter_forwards_path_and_stroke_methods() {
        use crate::draw::painter::Painter;
        use crate::draw::path::Path;

        let mut buf = vec![0u8; 32 * 32 * 4];
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, 32, 32);

        {
            let mut painter = Painter::new(&mut backend);
            let path = Path::rect(
                Fixed::from_int(4),
                Fixed::from_int(4),
                Fixed::from_int(10),
                Fixed::from_int(10),
            );
            painter.fill_path(&path, &clip, &Color::rgb(255, 0, 0), 255);

            painter.draw_line(
                Point {
                    x: Fixed::from_int(20),
                    y: Fixed::from_int(20),
                },
                Point {
                    x: Fixed::from_int(28),
                    y: Fixed::from_int(28),
                },
                &clip,
                Fixed::from_int(2),
                &Color::rgb(0, 255, 0),
                255,
            );

            painter.draw_arc(
                Point {
                    x: Fixed::from_int(24),
                    y: Fixed::from_int(8),
                },
                Fixed::from_int(4),
                Fixed::from_int(0),
                Fixed::from_int(90),
                &clip,
                Fixed::from_int(2),
                &Color::rgb(0, 0, 255),
                255,
            );
        }

        assert_eq!(backend.target.get_pixel(8, 8).r, 255);
        assert!(backend.target.get_pixel(24, 24).g > 0);
        assert!(
            backend.target.get_pixel(28, 8).b > 0
                || backend.target.get_pixel(27, 8).b > 0
                || backend.target.get_pixel(28, 9).b > 0,
        );
    }

    #[test]
    fn painter_draw_text_forwards_to_backend() {
        use crate::draw::painter::Painter;

        let mut buf = vec![0u8; 32 * 16 * 4];
        let tex = Texture::new(&mut buf, 32, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, 32, 16);

        {
            let mut painter = Painter::new(&mut backend);
            painter.draw_text(
                &Point {
                    x: Fixed::from_int(1),
                    y: Fixed::from_int(1),
                },
                b"B",
                &clip,
                &Color::rgb(200, 100, 50),
                255,
            );
        }

        let mut found = false;
        for y in 0..16 {
            for x in 0..32 {
                if backend.target.get_pixel(x, y).r > 0 {
                    found = true;
                    break;
                }
            }
        }
        assert!(found);
    }

    /// DDA and the divide-based slow path must land on the same src
    /// sample for every dst pixel. Drive both on the same 4×4 → 7×5
    /// non-integer scale and compare dst byte-for-byte.
    #[test]
    fn blit_dda_matches_generic_slow() {
        // Src: 4×4 ARGB with a distinct per-pixel red value so we can
        // tell which src pixel each dst pixel sampled.
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for y in 0..4 {
            for x in 0..4 {
                let i = (y * 4 + x) * 4;
                src_buf[i] = (y * 4 + x) as u8 * 16 + 1;
                src_buf[i + 3] = 255;
            }
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888);

        // Dst: 8×6, draw 7×5 blit at (0,0). Two identical dst buffers.
        let mut dst_a = vec![0u8; 8 * 6 * 4];
        let mut dst_b = vec![0u8; 8 * 6 * 4];

        {
            let mut tex_a = Texture::new(&mut dst_a, 8, 6, ColorFormat::RGBA8888);
            blit_generic_slow(&mut tex_a, &src, 0, 0, 4, 4, 0, 0, 7, 5, 0, 0, 8, 6);
        }
        {
            let mut tex_b = Texture::new(&mut dst_b, 8, 6, ColorFormat::RGBA8888);
            blit_dda(&mut tex_b, &src, 0, 0, 4, 4, 0, 0, 7, 5, 0, 0, 8, 6);
        }

        assert_eq!(dst_a, dst_b, "dda sampling diverged from divide path");
    }

    /// 1× fast path should match the slow path exactly on α==0 and
    /// α==255 pixels, and within ±1 per channel on partial-α blends
    /// (slow path uses Fixed map_range, fast path uses integer
    /// `(src * a + dst * (255 - a)) / 255` — the two differ by at
    /// most one LSB per channel).
    #[test]
    fn blit_1to1_matches_generic_for_argb_to_argb() {
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for i in 0..16 {
            src_buf[i * 4] = 30 + i as u8;
            src_buf[i * 4 + 1] = 60 + i as u8;
            src_buf[i * 4 + 2] = 90 + i as u8;
            src_buf[i * 4 + 3] = match i {
                0 => 0,
                3 | 7 => 128,
                _ => 255,
            };
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888);

        let mut dst_a = vec![0u8; 6 * 6 * 4];
        for (i, byte) in dst_a.iter_mut().enumerate() {
            *byte = (i * 3) as u8;
        }
        let mut dst_b = dst_a.clone();

        {
            let mut tex = Texture::new(&mut dst_a, 6, 6, ColorFormat::RGBA8888);
            blit_generic_slow(&mut tex, &src, 0, 0, 4, 4, 1, 1, 4, 4, 0, 0, 6, 6);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 6, 6, ColorFormat::RGBA8888);
            blit_1to1_fast(&mut tex, &src, 0, 0, 4, 4, 1, 1, 0, 0, 6, 6);
        }
        for (i, (&a, &b)) in dst_a.iter().zip(dst_b.iter()).enumerate() {
            assert!(
                (a as i32 - b as i32).abs() <= 1,
                "byte {} diverged by more than 1: slow={} fast={}",
                i,
                a,
                b
            );
        }
    }

    #[test]
    fn blit_1to1_matches_generic_for_argb_to_565sw() {
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for i in 0..16 {
            src_buf[i * 4] = 30 + i as u8 * 5;
            src_buf[i * 4 + 1] = 40 + i as u8 * 3;
            src_buf[i * 4 + 2] = 50 + i as u8 * 7;
            src_buf[i * 4 + 3] = if i == 0 { 0 } else { 255 };
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888);

        let mut dst_a = vec![0u8; 6 * 6 * 2];
        for i in 0..dst_a.len() {
            dst_a[i] = (i * 5) as u8;
        }
        let mut dst_b = dst_a.clone();

        {
            let mut tex = Texture::new(&mut dst_a, 6, 6, ColorFormat::RGB565Swapped);
            blit_generic_slow(&mut tex, &src, 0, 0, 4, 4, 1, 1, 4, 4, 0, 0, 6, 6);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 6, 6, ColorFormat::RGB565Swapped);
            blit_1to1_fast(&mut tex, &src, 0, 0, 4, 4, 1, 1, 0, 0, 6, 6);
        }
        assert_eq!(dst_a, dst_b);
    }

    /// 2× integer-scale fast path must produce the same 2×2 block
    /// pattern as the slow DDA path for each source pixel. Check
    /// 565sw→565sw (pure copy) for exact parity.
    #[test]
    fn blit_2to2_565sw_matches_dda() {
        let mut src_buf = vec![0u8; 3 * 3 * 2];
        for i in 0..9 {
            src_buf[i * 2] = 0x12 + i as u8;
            src_buf[i * 2 + 1] = 0x34 + i as u8;
        }
        let src = Texture::new(&mut src_buf, 3, 3, ColorFormat::RGB565Swapped);

        let mut dst_a = vec![0u8; 10 * 10 * 2];
        let mut dst_b = vec![0u8; 10 * 10 * 2];
        {
            let mut tex = Texture::new(&mut dst_a, 10, 10, ColorFormat::RGB565Swapped);
            blit_dda(&mut tex, &src, 0, 0, 3, 3, 1, 1, 6, 6, 0, 0, 10, 10);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 10, 10, ColorFormat::RGB565Swapped);
            blit_2to2_fast(&mut tex, &src, 0, 0, 3, 3, 1, 1, 0, 0, 10, 10);
        }
        assert_eq!(dst_a, dst_b);
    }

    /// 2× clip that lands on odd boundaries triggers the fast path's
    /// DDA fallback; verify the fallback still produces correct output.
    #[test]
    fn blit_2to2_odd_clip_falls_back_cleanly() {
        let mut src_buf = vec![0u8; 2 * 2 * 2];
        src_buf[0] = 0xAA;
        src_buf[1] = 0xBB;
        let src = Texture::new(&mut src_buf, 2, 2, ColorFormat::RGB565Swapped);

        let mut dst = vec![0u8; 6 * 6 * 2];
        let mut tex = Texture::new(&mut dst, 6, 6, ColorFormat::RGB565Swapped);
        // Clip starts at odd x=1: fast path 2×2 alignment broken → DDA fallback.
        blit_2to2_fast(&mut tex, &src, 0, 0, 2, 2, 0, 0, 1, 0, 6, 6);
        // Should not panic; column 0 stays zero, column 1+ has pixels.
        assert_eq!(dst[0], 0);
        assert_eq!(dst[1], 0);
        assert_ne!(dst[2], 0);
    }

    #[test]
    fn fill_rect_transformed_90deg_rotation() {
        let mut buf = vec![0u8; 16 * 16 * 4];
        let mut dst = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let rect = Rect::new(6, 6, 4, 4);
        let cx = rect.x + rect.w / Fixed::from_int(2);
        let cy = rect.y + rect.h / Fixed::from_int(2);
        let tf = Transform::translate(cx, cy)
            .compose(&Transform::rotate_deg(Fixed::from_int(90)))
            .compose(&Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy));
        let red = Color::rgb(255, 0, 0);
        fill_rect_transformed(&mut dst, rect, Rect::new(0, 0, 16, 16), &tf, &red, 255);

        let mut painted = 0;
        for y in 0..16 {
            for x in 0..16 {
                if dst.get_pixel(x, y).r == 255 {
                    painted += 1;
                }
            }
        }
        assert!(
            (12..=20).contains(&painted),
            "expected ~16 painted pixels, got {}",
            painted
        );
    }

    #[test]
    fn renderer_transformed_fill_uses_logical_area_under_hidpi() {
        let mut buf = vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = Viewport::new(32, 32, Fixed::from_int(2));

        renderer.draw(
            &DrawCommand::Fill {
                area: Rect::new(4, 4, 8, 8),
                transform: Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
                quad: None,
                color: Color::rgb(255, 0, 0),
                radius: Fixed::ZERO,
                opa: 255,
            },
            &Rect::new(0, 0, 32, 32),
        );

        assert_eq!(renderer.target.get_pixel(20, 20).r, 255);
        assert_eq!(renderer.target.get_pixel(10, 10).r, 0);
        assert_eq!(renderer.target.get_pixel(50, 50).r, 0);
    }

    #[test]
    fn blit_1to1_with_clip_restricted() {
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for i in 0..16 {
            src_buf[i * 4] = 100 + i as u8;
            src_buf[i * 4 + 3] = 255;
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888);

        let mut dst_a = vec![0u8; 8 * 8 * 4];
        let mut dst_b = vec![0u8; 8 * 8 * 4];

        // Clip covers only the right half of the blit rect.
        {
            let mut tex = Texture::new(&mut dst_a, 8, 8, ColorFormat::RGBA8888);
            blit_generic_slow(&mut tex, &src, 0, 0, 4, 4, 1, 1, 4, 4, 3, 0, 8, 8);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 8, 8, ColorFormat::RGBA8888);
            blit_1to1_fast(&mut tex, &src, 0, 0, 4, 4, 1, 1, 3, 0, 8, 8);
        }
        assert_eq!(dst_a, dst_b);
    }

    /// Clip partially covering the dst rect must still produce the
    /// same output as the divide-based slow path (every pixel outside
    /// the clip untouched, every pixel inside matching src sampling).
    #[test]
    fn blit_dda_with_partial_clip() {
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for i in 0..16 {
            src_buf[i * 4] = (i * 16 + 1) as u8;
            src_buf[i * 4 + 3] = 255;
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888);

        let mut dst_a = vec![0u8; 10 * 10 * 4];
        let mut dst_b = vec![0u8; 10 * 10 * 4];
        // Clip covers columns 2..7 only.
        {
            let mut tex_a = Texture::new(&mut dst_a, 10, 10, ColorFormat::RGBA8888);
            blit_generic_slow(&mut tex_a, &src, 0, 0, 4, 4, 1, 1, 8, 8, 2, 0, 7, 10);
        }
        {
            let mut tex_b = Texture::new(&mut dst_b, 10, 10, ColorFormat::RGBA8888);
            blit_dda(&mut tex_b, &src, 0, 0, 4, 4, 1, 1, 8, 8, 2, 0, 7, 10);
        }
        assert_eq!(dst_a, dst_b);
    }

    #[test]
    fn sw_renderer_supports_offscreen() {
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let backend = SwRenderer::new(tex);
        assert!(Renderer::supports_offscreen(&backend));
    }

    /// View `stride` must equal the framebuffer's so writing every
    /// pixel of the borrowed view hits exactly the rect's pixels in
    /// the underlying buffer.
    #[test]
    fn modify_target_region_writes_exact_rect() {
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(
            Fixed::from_int(4),
            Fixed::from_int(4),
            Fixed::from_int(6),
            Fixed::from_int(5),
        );
        let ran = backend.modify_target_region(&rect, &mut |view| {
            assert_eq!(view.width, 6);
            assert_eq!(view.height, 5);
            assert_eq!(view.stride, 16 * 4);
            for y in 0..view.height as i32 {
                for x in 0..view.width as i32 {
                    view.set_pixel(x, y, &Color::rgb(255, 128, 64));
                }
            }
        });
        assert!(ran);

        let target = &backend.target;
        for py in 0..16i32 {
            for px in 0..16i32 {
                let p = target.get_pixel(px, py);
                let in_rect = (4..10).contains(&px) && (4..9).contains(&py);
                if in_rect {
                    assert_eq!((p.r, p.g, p.b), (255, 128, 64), "in-rect ({px},{py})");
                } else {
                    assert_eq!((p.r, p.g, p.b), (0, 0, 0), "outside-rect ({px},{py})");
                }
            }
        }
    }
}
