use crate::types::{Color, Fixed, Point, Rect, Viewport};

use super::backend::DrawBackend;
use super::command::DrawCommand;
use super::path::Path;
use super::renderer::Renderer;
use super::texture::Texture;

#[cfg(feature = "perf")]
pub struct PerfCtx {
    pub clock: fn() -> u64,
    pub fill: u64,
    pub stroke: u64,
    pub blit: u64,
    pub label: u64,
    pub count_fill: u32,
    pub count_stroke: u32,
    pub count_blit: u32,
    pub count_label: u32,
}

#[cfg(feature = "perf")]
impl PerfCtx {
    pub fn new(clock: fn() -> u64) -> Self {
        Self {
            clock,
            fill: 0,
            stroke: 0,
            blit: 0,
            label: 0,
            count_fill: 0,
            count_stroke: 0,
            count_blit: 0,
            count_label: 0,
        }
    }

    pub fn reset(&mut self) {
        self.fill = 0;
        self.stroke = 0;
        self.blit = 0;
        self.label = 0;
        self.count_fill = 0;
        self.count_stroke = 0;
        self.count_blit = 0;
        self.count_label = 0;
    }
}

pub struct SwDrawBackend<'a> {
    pub target: Texture<'a>,
    pub viewport: Viewport,
    #[cfg(feature = "perf")]
    pub perf: Option<PerfCtx>,
}

impl<'a> SwDrawBackend<'a> {
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

impl<'a> SwDrawBackend<'a> {
    /// Scale every Point inside `path` into physical pixels so the
    /// rasterizer (which works in physical pixels) sees them directly.
    fn scale_path(&self, path: &Path) -> Path {
        let s = self.viewport.scale();
        let cmds = path
            .cmds
            .iter()
            .map(|c| match c {
                super::path::PathCmd::MoveTo(p) => super::path::PathCmd::MoveTo(Point {
                    x: p.x * s,
                    y: p.y * s,
                }),
                super::path::PathCmd::LineTo(p) => super::path::PathCmd::LineTo(Point {
                    x: p.x * s,
                    y: p.y * s,
                }),
                super::path::PathCmd::QuadTo { ctrl, end } => super::path::PathCmd::QuadTo {
                    ctrl: Point {
                        x: ctrl.x * s,
                        y: ctrl.y * s,
                    },
                    end: Point {
                        x: end.x * s,
                        y: end.y * s,
                    },
                },
                super::path::PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                    super::path::PathCmd::CubicTo {
                        ctrl1: Point {
                            x: ctrl1.x * s,
                            y: ctrl1.y * s,
                        },
                        ctrl2: Point {
                            x: ctrl2.x * s,
                            y: ctrl2.y * s,
                        },
                        end: Point {
                            x: end.x * s,
                            y: end.y * s,
                        },
                    }
                }
                super::path::PathCmd::Close => super::path::PathCmd::Close,
            })
            .collect();
        Path { cmds }
    }

    /// Rasterize an already-physical-coord path; used by stroke_path to
    /// avoid re-scaling the offset outline it already produced.
    fn fill_physical_path(&mut self, phys_path: &Path, clip: &Rect, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let segs = super::raster::flatten(phys_path);
        if segs.is_empty() {
            return;
        }
        let Some(bbox) = phys_path.bbox() else { return };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bbox
            .intersect(&phys_clip)
            .and_then(|r| r.intersect(&screen))
        else {
            return;
        };

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        super::raster::scanline_fill(&segs, px_x0, px_y0, px_x1, px_y1, |px, py, cov| {
            let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
            if final_alpha > 0 {
                self.target.blend_pixel_int(px, py, color, final_alpha);
            }
        });
    }
}

impl<'a> DrawBackend for SwDrawBackend<'a> {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }
        let phys_path = self.scale_path(path);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let segs = super::raster::flatten(&phys_path);
        if segs.is_empty() {
            return;
        }
        let Some(bbox) = phys_path.bbox() else { return };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bbox
            .intersect(&phys_clip)
            .and_then(|r| r.intersect(&screen))
        else {
            return;
        };

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        super::raster::scanline_fill(&segs, px_x0, px_y0, px_x1, px_y1, |px, py, cov| {
            let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
            if final_alpha > 0 {
                self.target.blend_pixel_int(px, py, color, final_alpha);
            }
        });
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_path = self.scale_path(path);
        let phys_width = width * self.viewport.scale();
        let outline = super::raster::offset_polygon(&phys_path, phys_width);
        // Outline is already physical — skip the usual fill_path scale step.
        self.fill_physical_path(&outline, clip, color, opa);
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let radius = radius * self.viewport.scale();
        let area = &phys_area;
        let clip = &phys_clip;

        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        if area.is_aligned() && r == Fixed::ZERO {
            if opa == 255 {
                let bpp = self.target.format.bytes_per_pixel();
                let buf = self.target.buf.as_mut_slice();
                let stride = self.target.stride;
                match self.target.format {
                    super::texture::ColorFormat::ARGB8888 => {
                        let pixel = [color.r, color.g, color.b, color.a];
                        for py in px_y0..px_y1 {
                            let row_start = py as usize * stride + px_x0 as usize * bpp;
                            for px in 0..(px_x1 - px_x0) as usize {
                                let i = row_start + px * 4;
                                buf[i..i + 4].copy_from_slice(&pixel);
                            }
                        }
                    }
                    super::texture::ColorFormat::RGB565
                    | super::texture::ColorFormat::RGB565Swapped => {
                        let px16 = ((color.r as u16 >> 3) << 11)
                            | ((color.g as u16 >> 2) << 5)
                            | (color.b as u16 >> 3);
                        let pixel =
                            if self.target.format == super::texture::ColorFormat::RGB565Swapped {
                                [(px16 >> 8) as u8, px16 as u8]
                            } else {
                                [px16 as u8, (px16 >> 8) as u8]
                            };
                        for py in px_y0..px_y1 {
                            let row_start = py as usize * stride + px_x0 as usize * bpp;
                            for px in 0..(px_x1 - px_x0) as usize {
                                let i = row_start + px * 2;
                                buf[i..i + 2].copy_from_slice(&pixel);
                            }
                        }
                    }
                    _ => {
                        for py in px_y0..px_y1 {
                            for px in px_x0..px_x1 {
                                self.target.set_pixel(px, py, color);
                            }
                        }
                    }
                }
            } else {
                for py in px_y0..px_y1 {
                    for px in px_x0..px_x1 {
                        self.target.blend_pixel_int(px, py, color, opa);
                    }
                }
            }
            return;
        }

        for py in px_y0..px_y1 {
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y = (pixel_bot.min(area.y + area.h) - pixel_top.max(area.y))
                .max(Fixed::ZERO)
                .min(Fixed::ONE);

            for px in px_x0..px_x1 {
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let cov_x = (pixel_right.min(area.x + area.w) - pixel_left.max(area.x))
                    .max(Fixed::ZERO)
                    .min(Fixed::ONE);

                let corner_cov = if r > Fixed::ZERO {
                    rounded_rect_coverage(
                        Fixed::from_int(px) - area.x,
                        Fixed::from_int(py) - area.y,
                        area.w,
                        area.h,
                        r,
                    )
                } else {
                    Fixed::ONE
                };

                let final_opa = (cov_x * cov_y * corner_cov * opa_norm).map01(255).to_int() as u8;
                if final_opa > 0 {
                    self.target.blend_pixel(
                        Fixed::from_int(px),
                        Fixed::from_int(py),
                        color,
                        final_opa,
                    );
                }
            }
        }
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
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let bw = width;
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        let inner_r = (r - bw).max(Fixed::ZERO);
        let inner_w = (area.w - bw * 2).max(Fixed::ZERO);
        let inner_h = (area.h - bw * 2).max(Fixed::ZERO);

        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                let rel_x = Fixed::from_int(px) - area.x;
                let rel_y = Fixed::from_int(py) - area.y;

                let outer_cov = rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r);
                if outer_cov == Fixed::ZERO {
                    continue;
                }

                let inner_rel_x = rel_x - bw;
                let inner_rel_y = rel_y - bw;
                let inner_cov = if inner_rel_x >= Fixed::ZERO
                    && inner_rel_y >= Fixed::ZERO
                    && inner_rel_x < inner_w
                    && inner_rel_y < inner_h
                {
                    rounded_rect_coverage(inner_rel_x, inner_rel_y, inner_w, inner_h, inner_r)
                } else {
                    Fixed::ZERO
                };

                let border_cov = (outer_cov - inner_cov).max(Fixed::ZERO);
                let final_opa = (border_cov * opa_norm).map01(255).to_int() as u8;
                if final_opa > 0 {
                    self.target.blend_pixel(
                        Fixed::from_int(px),
                        Fixed::from_int(py),
                        color,
                        final_opa,
                    );
                }
            }
        }
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
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
        // Runtime dispatch: 1× / 2× / arbitrary. All three branches
        // currently resolve to the same slow path; follow-up commits
        // swap each in for a specialized implementation.
        #[allow(clippy::if_same_then_else)]
        if dw == sw_i && dh == sh_i {
            blit_generic_slow(
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
        } else if dw == sw_i * 2 && dh == sh_i * 2 {
            blit_generic_slow(
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
        } else {
            blit_generic_slow(
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
        use super::font::{CHAR_H, CHAR_W, glyph};
        let phys_pos = self.viewport.point_to_physical(*pos);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_clip.pixel_bounds();
        let (mut cx, cy) = phys_pos.floor();
        for &ch in text {
            let bitmap = glyph(ch);
            for row in 0..CHAR_H as i32 {
                let byte = bitmap[row as usize];
                for col in 0..CHAR_W as i32 {
                    if byte & (0x80 >> col) != 0 {
                        let px = cx + col;
                        let py = cy + row;
                        if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                            self.target.blend_pixel(
                                Fixed::from_int(px),
                                Fixed::from_int(py),
                                color,
                                opa,
                            );
                        }
                    }
                }
            }
            cx += CHAR_W as i32;
        }
    }

    fn flush(&mut self) {}
}

/// Generic nearest-neighbour blit. Currently the sole implementation;
/// follow-up commits add 1× / 2× integer-scale + format-specialized
/// variants and route the hot paths here only as fallback.
#[allow(clippy::too_many_arguments)]
fn blit_generic_slow(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    dw: i32,
    dh: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    for drow in 0..dh {
        let iy = dy0 + drow;
        if iy < clip_y0 || iy >= clip_y1 {
            continue;
        }
        let sy = sy0 + (drow * sh as i32) / dh;
        for dcol in 0..dw {
            let ix = dx0 + dcol;
            if ix < clip_x0 || ix >= clip_x1 {
                continue;
            }
            let sx = sx0 + (dcol * sw as i32) / dw;
            let src_color = src.get_pixel(sx, sy);
            if src_color.a == 0 {
                continue;
            }
            if src_color.a == 255 {
                dst.set_pixel(ix, iy, &src_color);
            } else {
                dst.blend_pixel_int(ix, iy, &src_color, src_color.a);
            }
        }
    }
}

fn rounded_rect_coverage(px: Fixed, py: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Fixed {
    if r == Fixed::ZERO {
        return Fixed::ONE;
    }

    let (cx, cy) = if px < r && py < r {
        (r, r)
    } else if px >= w - r && py < r {
        (w - r, r)
    } else if px < r && py >= h - r {
        (r, h - r)
    } else if px >= w - r && py >= h - r {
        (w - r, h - r)
    } else {
        return Fixed::ONE;
    };

    let dx = px - cx + Fixed::ONE / 2;
    let dy = py - cy + Fixed::ONE / 2;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq <= r * r {
        Fixed::ONE
    } else {
        let dist = dist_sq.sqrt();
        let overshoot = dist - r;
        if overshoot >= Fixed::ONE {
            Fixed::ZERO
        } else {
            Fixed::ONE - overshoot
        }
    }
}

impl Renderer for SwDrawBackend<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        assert!(
            cmd.transform().is_identity(),
            "widget transform not yet supported — tracked in widget-transform spec"
        );
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                self.fill_rect(area, clip, color, *radius, *opa);
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.fill += (p.clock)() - t0;
                    p.count_fill += 1;
                }
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                self.stroke_rect(area, clip, *width, color, *radius, *opa);
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.stroke += (p.clock)() - t0;
                    p.count_stroke += 1;
                }
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit(texture, &src_rect, *pos, *size, clip);
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.blit += (p.clock)() - t0;
                    p.count_blit += 1;
                }
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                self.draw_label(pos, text, clip, color, *opa);
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.label += (p.clock)() - t0;
                    p.count_label += 1;
                }
            }
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
                ..
            } => {
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                self.draw_line(*p1, *p2, clip, *width, color, *opa);
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.stroke += (p.clock)() - t0;
                    p.count_stroke += 1;
                }
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
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                self.draw_arc(
                    *center,
                    *radius,
                    *start_angle,
                    *end_angle,
                    clip,
                    *width,
                    color,
                    *opa,
                );
                #[cfg(feature = "perf")]
                if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
                    p.stroke += (p.clock)() - t0;
                    p.count_stroke += 1;
                }
            }
        }
    }

    fn flush(&mut self) {
        DrawBackend::flush(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::texture::ColorFormat;
    use alloc::vec;

    #[test]
    fn fill_rect_basic() {
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        // just verifies the method exists on DrawBackend and writes something.
        let mut buf = vec![0u8; 32 * 16 * 4];
        let tex = Texture::new(&mut buf, 32, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

        let pos = Point {
            x: Fixed::from_int(1),
            y: Fixed::from_int(1),
        };
        let clip = Rect::new(0, 0, 32, 16);
        DrawBackend::draw_label(&mut backend, &pos, b"A", &clip, &Color::rgb(255, 0, 0), 255);

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
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        // Exercises DrawBackend::draw_line's default trait impl → stroke_path.
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);

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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);
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
        let tex = Texture::new(&mut buf, 32, 16, ColorFormat::ARGB8888);
        let mut backend = SwDrawBackend::new(tex);
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
}
