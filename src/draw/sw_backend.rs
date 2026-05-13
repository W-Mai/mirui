use crate::types::{Color, Fixed, Fixed64, Point, Rect, Transform, Viewport};

use super::backend::DrawBackend;
use super::command::DrawCommand;
use super::path::Path;
use super::renderer::Renderer;
use super::texture::{ColorFormat, Texture};

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

/// Global perf counters for quad-path drawing. Not thread-safe (plain
/// `static mut`), which matches the single-threaded embedded targets mirui
/// runs on. Off-by-default: zero cost when `perf` feature is off.
#[cfg(feature = "perf")]
pub mod quad_perf {
    pub static mut FILL: u64 = 0;
    pub static mut BLIT: u64 = 0;
    pub static mut FILL_COUNT: u32 = 0;
    pub static mut BLIT_COUNT: u32 = 0;

    /// Per-pixel breakdown for fill_rect_quad bbox scan.
    pub static mut FILL_PIXELS_SCANNED: u64 = 0;
    pub static mut FILL_PIXELS_DRAWN: u64 = 0;
    pub static mut FILL_PIXELS_INSET_HIT: u64 = 0;
    pub static mut FILL_PIXELS_SLOW_HIT: u64 = 0;

    /// Per-pixel breakdown for blit_quad.
    pub static mut BLIT_PIXELS_SCANNED: u64 = 0;
    pub static mut BLIT_PIXELS_DRAWN: u64 = 0;

    /// User supplies a clock reading monotonic ticks. Demo points it at
    /// e.g. ESP systimer (cycles) or std Instant (ns) — caller decides
    /// units and interprets the output accordingly.
    pub static mut CLOCK: fn() -> u64 = || 0;

    pub struct Snapshot {
        pub fill_ticks: u64,
        pub fill_count: u32,
        pub blit_ticks: u64,
        pub blit_count: u32,
        pub fill_scanned: u64,
        pub fill_drawn: u64,
        pub fill_inset_hit: u64,
        pub fill_slow_hit: u64,
        pub blit_scanned: u64,
        pub blit_drawn: u64,
    }

    pub fn take() -> Snapshot {
        unsafe {
            let out = Snapshot {
                fill_ticks: FILL,
                fill_count: FILL_COUNT,
                blit_ticks: BLIT,
                blit_count: BLIT_COUNT,
                fill_scanned: FILL_PIXELS_SCANNED,
                fill_drawn: FILL_PIXELS_DRAWN,
                fill_inset_hit: FILL_PIXELS_INSET_HIT,
                fill_slow_hit: FILL_PIXELS_SLOW_HIT,
                blit_scanned: BLIT_PIXELS_SCANNED,
                blit_drawn: BLIT_PIXELS_DRAWN,
            };
            FILL = 0;
            BLIT = 0;
            FILL_COUNT = 0;
            BLIT_COUNT = 0;
            FILL_PIXELS_SCANNED = 0;
            FILL_PIXELS_DRAWN = 0;
            FILL_PIXELS_INSET_HIT = 0;
            FILL_PIXELS_SLOW_HIT = 0;
            BLIT_PIXELS_SCANNED = 0;
            BLIT_PIXELS_DRAWN = 0;
            out
        }
    }

    #[inline]
    pub fn now() -> u64 {
        unsafe { CLOCK() }
    }

    #[inline]
    pub fn add_fill(dt: u64) {
        unsafe {
            FILL += dt;
            FILL_COUNT += 1;
        }
    }

    #[inline]
    pub fn add_blit(dt: u64) {
        unsafe {
            BLIT += dt;
            BLIT_COUNT += 1;
        }
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

    fn draw_transformed(&mut self, cmd: &DrawCommand, clip: &Rect, tf: &Transform) {
        let vp = self.viewport.as_transform();
        let phys_tf = vp.compose(tf);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        match cmd {
            DrawCommand::Fill {
                area, color, opa, ..
            } => {
                let phys_area = self.viewport.rect_to_physical(*area);
                fill_rect_transformed(
                    &mut self.target,
                    phys_area,
                    phys_clip,
                    &phys_tf,
                    color,
                    *opa,
                );
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                let phys_pos = self.viewport.point_to_physical(*pos);
                let phys_size = self.viewport.point_to_physical(*size);
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let phys_dst = Rect {
                    x: phys_pos.x,
                    y: phys_pos.y,
                    w: phys_size.x,
                    h: phys_size.y,
                };
                blit_transformed(
                    &mut self.target,
                    texture,
                    &src_rect,
                    phys_dst,
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
        // Runtime dispatch: 1× / 2× / arbitrary. 1× goes to the
        // format-specialized fast variant; arbitrary goes to DDA;
        // 2× still on the old slow path until the next commit.
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

/// Rasterize a solid-colour rect under an arbitrary transform by
/// inverse-sampling every pixel in the transformed bbox. Caller is
/// responsible for supplying the physical-space transform and clip.
fn quad_bbox(q: &[Point; 4]) -> Rect {
    let mut min_x = q[0].x;
    let mut max_x = q[0].x;
    let mut min_y = q[0].y;
    let mut max_y = q[0].y;
    for p in &q[1..] {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

fn blit_quad(dst: &mut Texture, src: &Texture, q: &[Point; 4], phys_clip: Rect) {
    use crate::types::Transform3D;
    let src_rect = Rect::new(0, 0, src.width, src.height);
    let Some(forward) = Transform3D::from_quad(src_rect, q) else {
        return;
    };
    let Some(inverse) = forward.inverse() else {
        return;
    };
    let bbox = quad_bbox(q);
    let Some(area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = area.pixel_bounds();
    let sw = src.width as i32;
    let sh = src.height as i32;
    let half = Fixed64::from_raw(Fixed64::ONE.raw() >> 1);
    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_l, x_r)) = quad_row_span(q, py_f) else {
            continue;
        };
        let x_l_px = x_l.to_int().max(px_x0);
        let x_r_px = x_r.ceil().to_int().min(px_x1);
        if x_r_px <= x_l_px {
            continue;
        }
        // DDA starting point at (x_l_px + 0.5, py_f).
        let x0_f = Fixed64::from_fixed(Fixed::from_int(x_l_px)) + half;
        let y0_f = Fixed64::from_fixed(py_f);
        let mut big_x = inverse.m00 * x0_f + inverse.m01 * y0_f + inverse.m02;
        let mut big_y = inverse.m10 * x0_f + inverse.m11 * y0_f + inverse.m12;
        let mut w = inverse.m20 * x0_f + inverse.m21 * y0_f + inverse.m22;
        #[cfg(feature = "perf")]
        unsafe {
            quad_perf::BLIT_PIXELS_SCANNED += (x_r_px - x_l_px) as u64;
        }
        for px in x_l_px..x_r_px {
            if w.raw() > 0 {
                let u = (big_x / w).to_fixed();
                let v = (big_y / w).to_fixed();
                let sx = u.to_int();
                let sy = v.to_int();
                if sx >= 0 && sx < sw && sy >= 0 && sy < sh {
                    let c = src.get_pixel(sx, sy);
                    if c.a != 0 {
                        #[cfg(feature = "perf")]
                        unsafe {
                            quad_perf::BLIT_PIXELS_DRAWN += 1;
                        }
                        if c.a == 255 {
                            dst.set_pixel(px, py, &c);
                        } else {
                            dst.blend_pixel_int(px, py, &c, c.a);
                        }
                    }
                }
            }
            big_x += inverse.m00;
            big_y += inverse.m10;
            w += inverse.m20;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect_quad(
    dst: &mut Texture,
    q: &[Point; 4],
    phys_clip: Rect,
    color: &Color,
    radius: Fixed,
    local_w: Fixed,
    local_h: Fixed,
    opa: u8,
) {
    use crate::types::transform_3d::point_in_quad;
    let bbox = quad_bbox(q);
    let Some(area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = area.pixel_bounds();

    if radius == Fixed::ZERO {
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                let p = Point {
                    x: Fixed::from_int(px) + Fixed::from_raw(128),
                    y: Fixed::from_int(py) + Fixed::from_raw(128),
                };
                if !point_in_quad(q, p) {
                    continue;
                }
                if opa == 255 {
                    dst.set_pixel(px, py, color);
                } else {
                    dst.blend_pixel_int(px, py, color, opa);
                }
            }
        }
        return;
    }

    let _ = (local_w, local_h);
    let corners = build_corner_info(q, radius);
    let r_sq = radius * radius;
    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_l, x_r)) = quad_row_span(q, py_f) else {
            continue;
        };
        let x_l_px = x_l.to_int().max(px_x0);
        let x_r_px = x_r.ceil().to_int().min(px_x1);
        if x_r_px <= x_l_px {
            continue;
        }
        #[cfg(feature = "perf")]
        unsafe {
            quad_perf::FILL_PIXELS_SCANNED += (x_r_px - x_l_px) as u64;
        }
        for px in x_l_px..x_r_px {
            let p = Point {
                x: Fixed::from_int(px) + Fixed::from_raw(128),
                y: py_f,
            };
            if pixel_clipped_by_corner(&corners, p, r_sq) {
                continue;
            }
            #[cfg(feature = "perf")]
            unsafe {
                quad_perf::FILL_PIXELS_DRAWN += 1;
            }
            if opa == 255 {
                dst.set_pixel(px, py, color);
            } else {
                dst.blend_pixel_int(px, py, color, opa);
            }
        }
    }
}

struct CornerInfo {
    centre: Point,
    // Inward unit vectors along the two incident edges, scaled so that
    // a projection >= 0 means "toward polygon interior along that edge".
    inward_a: (Fixed, Fixed),
    inward_b: (Fixed, Fixed),
}

fn build_corner_info(q: &[Point; 4], r: Fixed) -> [CornerInfo; 4] {
    let mut out = [
        CornerInfo {
            centre: Point::ZERO,
            inward_a: (Fixed::ZERO, Fixed::ZERO),
            inward_b: (Fixed::ZERO, Fixed::ZERO),
        },
        CornerInfo {
            centre: Point::ZERO,
            inward_a: (Fixed::ZERO, Fixed::ZERO),
            inward_b: (Fixed::ZERO, Fixed::ZERO),
        },
        CornerInfo {
            centre: Point::ZERO,
            inward_a: (Fixed::ZERO, Fixed::ZERO),
            inward_b: (Fixed::ZERO, Fixed::ZERO),
        },
        CornerInfo {
            centre: Point::ZERO,
            inward_a: (Fixed::ZERO, Fixed::ZERO),
            inward_b: (Fixed::ZERO, Fixed::ZERO),
        },
    ];
    for i in 0..4 {
        let vertex = q[i];
        let next = q[(i + 1) % 4];
        let prev = q[(i + 3) % 4];
        let e1x = next.x - vertex.x;
        let e1y = next.y - vertex.y;
        let l1 = (e1x * e1x + e1y * e1y).sqrt();
        let e2x = prev.x - vertex.x;
        let e2y = prev.y - vertex.y;
        let l2 = (e2x * e2x + e2y * e2y).sqrt();
        let (ux, uy) = if l1 > Fixed::ZERO {
            (e1x / l1, e1y / l1)
        } else {
            (Fixed::ZERO, Fixed::ZERO)
        };
        let (vx, vy) = if l2 > Fixed::ZERO {
            (e2x / l2, e2y / l2)
        } else {
            (Fixed::ZERO, Fixed::ZERO)
        };
        out[i] = CornerInfo {
            centre: Point {
                x: vertex.x + ux * r + vx * r,
                y: vertex.y + uy * r + vy * r,
            },
            inward_a: (ux, uy),
            inward_b: (vx, vy),
        };
    }
    out
}

/// Pixel is clipped by a corner iff it is in that corner's outward wedge
/// (both edge projections negative, i.e. past the corner vertex in both
/// directions) AND farther than r from the corner centre.
fn pixel_clipped_by_corner(corners: &[CornerInfo; 4], p: Point, r_sq: Fixed) -> bool {
    for c in corners {
        let dx = p.x - c.centre.x;
        let dy = p.y - c.centre.y;
        // Project dx,dy onto the two inward directions. If BOTH projections
        // are negative, pixel is in the outward wedge of this corner.
        let proj_a = dx * c.inward_a.0 + dy * c.inward_a.1;
        let proj_b = dx * c.inward_b.0 + dy * c.inward_b.1;
        if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > r_sq {
                return true;
            }
        }
    }
    false
}

/// Intersect horizontal line y=py with convex quad q; return leftmost and
/// rightmost x of the two intersections. None if row is fully outside.
fn quad_row_span(q: &[Point; 4], py: Fixed) -> Option<(Fixed, Fixed)> {
    let mut x_l = Fixed::MAX;
    let mut x_r = Fixed::MIN;
    let mut hit = false;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let dy = b.y - a.y;
        // Skip horizontal edges (no crossing or coincident).
        if dy.raw() == 0 {
            continue;
        }
        // Does y=py cross this edge? Need a.y <= py < b.y OR b.y <= py < a.y.
        let (y0, y1) = if dy.raw() > 0 { (a.y, b.y) } else { (b.y, a.y) };
        if py < y0 || py >= y1 {
            continue;
        }
        // Linear interp: x = a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x)
        // Use Fixed64 to avoid Q24.8 mul overflow on large widgets.
        let dx = Fixed64::from_fixed(b.x - a.x);
        let t_num = Fixed64::from_fixed(py - a.y);
        let t_den = Fixed64::from_fixed(dy);
        let x = Fixed64::from_fixed(a.x) + t_num * dx / t_den;
        let x = x.to_fixed();
        if x < x_l {
            x_l = x;
        }
        if x > x_r {
            x_r = x;
        }
        hit = true;
    }
    if hit { Some((x_l, x_r)) } else { None }
}

fn fill_rect_transformed(
    dst: &mut Texture,
    phys_rect: Rect,
    phys_clip: Rect,
    tf: &Transform,
    color: &Color,
    opa: u8,
) {
    let Some(inv) = tf.inverse() else { return };
    let bbox = tf.apply_rect_bbox(phys_rect);
    let Some(draw_area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(draw_area) = draw_area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
    let rx0 = phys_rect.x;
    let ry0 = phys_rect.y;
    let rx1 = phys_rect.x + phys_rect.w;
    let ry1 = phys_rect.y + phys_rect.h;
    for py in px_y0..px_y1 {
        for px in px_x0..px_x1 {
            let sample = inv.apply_point(Point {
                x: Fixed::from_int(px) + Fixed::from_raw(128),
                y: Fixed::from_int(py) + Fixed::from_raw(128),
            });
            if sample.x < rx0 || sample.x >= rx1 || sample.y < ry0 || sample.y >= ry1 {
                continue;
            }
            if opa == 255 {
                dst.set_pixel(px, py, color);
            } else {
                dst.blend_pixel_int(px, py, color, opa);
            }
        }
    }
}

/// Texture blit under an arbitrary transform. Uses nearest-neighbour
/// inverse sampling; matches the existing identity blit sampling
/// semantics so rotating a sprite 0° degenerates to the old output.
#[allow(clippy::too_many_arguments)]
fn blit_transformed(
    dst: &mut Texture,
    src: &Texture,
    src_rect: &Rect,
    phys_dst_rect: Rect,
    phys_clip: Rect,
    tf: &Transform,
) {
    let Some(inv) = tf.inverse() else { return };
    let bbox = tf.apply_rect_bbox(phys_dst_rect);
    let Some(draw_area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(draw_area) = draw_area.intersect(&screen) else {
        return;
    };
    let (dx0, dy0, dx1, dy1) = draw_area.pixel_bounds();

    let (sx0, sy0, sw, sh) = src_rect.to_px();
    let dst_x0 = phys_dst_rect.x;
    let dst_y0 = phys_dst_rect.y;
    let dst_w = phys_dst_rect.w;
    let dst_h = phys_dst_rect.h;
    if dst_w <= Fixed::ZERO || dst_h <= Fixed::ZERO || sw == 0 || sh == 0 {
        return;
    }

    for py in dy0..dy1 {
        for px in dx0..dx1 {
            let dp = inv.apply_point(Point {
                x: Fixed::from_int(px) + Fixed::from_raw(128),
                y: Fixed::from_int(py) + Fixed::from_raw(128),
            });
            let u = dp.x - dst_x0;
            let v = dp.y - dst_y0;
            if u < Fixed::ZERO || v < Fixed::ZERO || u >= dst_w || v >= dst_h {
                continue;
            }
            let sx = sx0 + (u * Fixed::from_int(sw as i32) / dst_w).to_int();
            let sy = sy0 + (v * Fixed::from_int(sh as i32) / dst_h).to_int();
            if sx < sx0 || sx >= sx0 + sw as i32 || sy < sy0 || sy >= sy0 + sh as i32 {
                continue;
            }
            let c = src.get_pixel(sx, sy);
            if c.a == 0 {
                continue;
            }
            if c.a == 255 {
                dst.set_pixel(px, py, &c);
            } else {
                dst.blend_pixel_int(px, py, &c, c.a);
            }
        }
    }
}

#[inline]
fn offset_rect(r: &Rect, tx: Fixed, ty: Fixed) -> Rect {
    if tx == Fixed::ZERO && ty == Fixed::ZERO {
        return *r;
    }
    Rect {
        x: r.x + tx,
        y: r.y + ty,
        w: r.w,
        h: r.h,
    }
}

#[inline]
fn offset_point(p: &Point, tx: Fixed, ty: Fixed) -> Point {
    Point {
        x: p.x + tx,
        y: p.y + ty,
    }
}

/// Generic nearest-neighbour blit kept as an out-of-line fallback.
/// Uses the original per-pixel divide; superseded by `blit_dda`
/// everywhere except places that intentionally want this shape.
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

/// 1× integer-scale blit: dst rect exactly matches src rect size.
/// Specialized per (src_format, dst_format) so the inner loop is a
/// tight byte copy / format convert with no `get_pixel` / `set_pixel`
/// bookkeeping. Caller must ensure (dst_x, dst_y, dst_x+sw, dst_y+sh)
/// sits entirely inside clip — S1 dispatch verifies this before
/// calling us; anything else falls back to `blit_dda`.
#[allow(clippy::too_many_arguments)]
fn blit_1to1_fast(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    // Restrict to the visible dst subrect (dst ∩ clip). Everything
    // inside is guaranteed to land within the dst texture since the
    // entry in blit() already intersected with screen + dst bounds.
    let vx0 = dx0.max(clip_x0);
    let vy0 = dy0.max(clip_y0);
    let vx1 = (dx0 + sw as i32).min(clip_x1);
    let vy1 = (dy0 + sh as i32).min(clip_y1);
    if vx1 <= vx0 || vy1 <= vy0 {
        return;
    }
    let src_x0 = sx0 + (vx0 - dx0);
    let src_y0 = sy0 + (vy0 - dy0);
    let run_w = (vx1 - vx0) as usize;
    let run_h = (vy1 - vy0) as usize;

    match (src.format, dst.format) {
        (ColorFormat::ARGB8888, ColorFormat::ARGB8888) => {
            blit_1to1_argb_to_argb(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::ARGB8888, ColorFormat::RGB565Swapped) => {
            blit_1to1_argb_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::RGB565Swapped, ColorFormat::RGB565Swapped) => {
            blit_1to1_565sw_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::ARGB8888, ColorFormat::RGB565) => {
            blit_1to1_argb_to_565(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        _ => blit_generic_slow(
            dst,
            src,
            src_x0,
            src_y0,
            run_w as u16,
            run_h as u16,
            vx0,
            vy0,
            run_w as i32,
            run_h as i32,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_argb(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 4;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            if a == 255 {
                dst_buf[di] = src_buf[si];
                dst_buf[di + 1] = src_buf[si + 1];
                dst_buf[di + 2] = src_buf[si + 2];
                dst_buf[di + 3] = 255;
            } else {
                // Blend: out = src * a + dst * (255 - a), via integer.
                let inv = 255 - a as u16;
                let sa = a as u16;
                dst_buf[di] = ((src_buf[si] as u16 * sa + dst_buf[di] as u16 * inv) / 255) as u8;
                dst_buf[di + 1] =
                    ((src_buf[si + 1] as u16 * sa + dst_buf[di + 1] as u16 * inv) / 255) as u8;
                dst_buf[di + 2] =
                    ((src_buf[si + 2] as u16 * sa + dst_buf[di + 2] as u16 * inv) / 255) as u8;
                dst_buf[di + 3] = 255;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 2;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let (r, g, b) = if a == 255 {
                (src_buf[si], src_buf[si + 1], src_buf[si + 2])
            } else {
                // Decode existing dst 565 → RGB888, blend, re-encode.
                let hi = dst_buf[di] as u16;
                let lo = dst_buf[di + 1] as u16;
                let px = lo | (hi << 8);
                let dr = ((px >> 11) as u8) << 3;
                let dg = (((px >> 5) & 0x3F) as u8) << 2;
                let db = ((px & 0x1F) as u8) << 3;
                let inv = 255 - a as u16;
                let sa = a as u16;
                (
                    ((src_buf[si] as u16 * sa + dr as u16 * inv) / 255) as u8,
                    ((src_buf[si + 1] as u16 * sa + dg as u16 * inv) / 255) as u8,
                    ((src_buf[si + 2] as u16 * sa + db as u16 * inv) / 255) as u8,
                )
            };
            let px = ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
            dst_buf[di] = (px >> 8) as u8;
            dst_buf[di + 1] = px as u8;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_565sw_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    // 565 has no alpha channel — always fully opaque copy. Use
    // copy_from_slice per row for the best chance of memcpy in
    // release builds.
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 2;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        dst_buf[dst_row_off..dst_row_off + run_w * 2]
            .copy_from_slice(&src_buf[src_row_off..src_row_off + run_w * 2]);
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_565(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    // Identical logic to 565sw variant except low/high byte order is
    // swapped on encode. Separate functions so the format is a
    // compile-time constant within each.
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 2;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let (r, g, b) = if a == 255 {
                (src_buf[si], src_buf[si + 1], src_buf[si + 2])
            } else {
                let lo = dst_buf[di] as u16;
                let hi = dst_buf[di + 1] as u16;
                let px = lo | (hi << 8);
                let dr = ((px >> 11) as u8) << 3;
                let dg = (((px >> 5) & 0x3F) as u8) << 2;
                let db = ((px & 0x1F) as u8) << 3;
                let inv = 255 - a as u16;
                let sa = a as u16;
                (
                    ((src_buf[si] as u16 * sa + dr as u16 * inv) / 255) as u8,
                    ((src_buf[si + 1] as u16 * sa + dg as u16 * inv) / 255) as u8,
                    ((src_buf[si + 2] as u16 * sa + db as u16 * inv) / 255) as u8,
                )
            };
            let px = ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
            dst_buf[di] = px as u8;
            dst_buf[di + 1] = (px >> 8) as u8;
        }
    }
}

/// 2× integer-scale blit: dst is exactly twice src in both axes.
/// Each src pixel writes a 2×2 dst block; src is read once per block
/// instead of 4 times in the generic path. Specialized per
/// (src_format, dst_format) like `blit_1to1_fast`.
#[allow(clippy::too_many_arguments)]
fn blit_2to2_fast(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    // Clip to dst rect ∩ clip. 2× constraint requires the visible
    // rect to start at even offsets from (dx0, dy0) for fast path;
    // anything else falls back to DDA which handles it correctly.
    let vx0 = dx0.max(clip_x0);
    let vy0 = dy0.max(clip_y0);
    let vx1 = (dx0 + (sw as i32) * 2).min(clip_x1);
    let vy1 = (dy0 + (sh as i32) * 2).min(clip_y1);
    if vx1 <= vx0 || vy1 <= vy0 {
        return;
    }
    // A dst pixel at (vx0, vy0) corresponds to src pixel
    // (sx0 + (vx0 - dx0) / 2, sy0 + (vy0 - dy0) / 2). If the visible
    // rect doesn't land on even boundaries the block structure
    // breaks; the DDA path is correct so we fall through there.
    let dx_off = vx0 - dx0;
    let dy_off = vy0 - dy0;
    if dx_off & 1 != 0 || dy_off & 1 != 0 || (vx1 - vx0) & 1 != 0 || (vy1 - vy0) & 1 != 0 {
        blit_dda(
            dst,
            src,
            sx0,
            sy0,
            sw,
            sh,
            dx0,
            dy0,
            (sw as i32) * 2,
            (sh as i32) * 2,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        );
        return;
    }

    let src_x0 = sx0 + dx_off / 2;
    let src_y0 = sy0 + dy_off / 2;
    let block_w = (vx1 - vx0) as usize / 2;
    let block_h = (vy1 - vy0) as usize / 2;

    match (src.format, dst.format) {
        (ColorFormat::ARGB8888, ColorFormat::ARGB8888) => {
            blit_2to2_argb_to_argb(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::ARGB8888, ColorFormat::RGB565Swapped) => {
            blit_2to2_argb_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::RGB565Swapped, ColorFormat::RGB565Swapped) => {
            blit_2to2_565sw_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::ARGB8888, ColorFormat::RGB565) => {
            blit_2to2_argb_to_565(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        _ => blit_dda(
            dst,
            src,
            sx0,
            sy0,
            sw,
            sh,
            dx0,
            dy0,
            (sw as i32) * 2,
            (sh as i32) * 2,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_argb(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 4;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let px = [src_buf[si], src_buf[si + 1], src_buf[si + 2], a];
            if a == 255 {
                let d00 = dst_row0 + col * 8;
                let d01 = d00 + 4;
                let d10 = dst_row1 + col * 8;
                let d11 = d10 + 4;
                dst_buf[d00..d00 + 4].copy_from_slice(&px);
                dst_buf[d01..d01 + 4].copy_from_slice(&px);
                dst_buf[d10..d10 + 4].copy_from_slice(&px);
                dst_buf[d11..d11 + 4].copy_from_slice(&px);
            } else {
                // Four blends; hand-roll instead of a helper to keep
                // bounds check elided by the compiler per index.
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 8,
                    dst_row0 + col * 8 + 4,
                    dst_row1 + col * 8,
                    dst_row1 + col * 8 + 4,
                ] {
                    dst_buf[di] = ((px[0] as u16 * sa + dst_buf[di] as u16 * inv) / 255) as u8;
                    dst_buf[di + 1] =
                        ((px[1] as u16 * sa + dst_buf[di + 1] as u16 * inv) / 255) as u8;
                    dst_buf[di + 2] =
                        ((px[2] as u16 * sa + dst_buf[di + 2] as u16 * inv) / 255) as u8;
                    dst_buf[di + 3] = 255;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            // α==255 path: encode once, splat 4 times.
            if a == 255 {
                let r = src_buf[si] as u16;
                let g = src_buf[si + 1] as u16;
                let b = src_buf[si + 2] as u16;
                let px = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                let hi = (px >> 8) as u8;
                let lo = px as u8;
                let d00 = dst_row0 + col * 4;
                let d10 = dst_row1 + col * 4;
                dst_buf[d00] = hi;
                dst_buf[d00 + 1] = lo;
                dst_buf[d00 + 2] = hi;
                dst_buf[d00 + 3] = lo;
                dst_buf[d10] = hi;
                dst_buf[d10 + 1] = lo;
                dst_buf[d10 + 2] = hi;
                dst_buf[d10 + 3] = lo;
            } else {
                // Partial α on 565 loses precision either way. Use
                // the generic 1-pixel helper 4 times; not hot.
                let sr = src_buf[si];
                let sg = src_buf[si + 1];
                let sb = src_buf[si + 2];
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 4,
                    dst_row0 + col * 4 + 2,
                    dst_row1 + col * 4,
                    dst_row1 + col * 4 + 2,
                ] {
                    let hi = dst_buf[di] as u16;
                    let lo = dst_buf[di + 1] as u16;
                    let dpx = lo | (hi << 8);
                    let dr = ((dpx >> 11) as u8) << 3;
                    let dg = (((dpx >> 5) & 0x3F) as u8) << 2;
                    let db = ((dpx & 0x1F) as u8) << 3;
                    let br = ((sr as u16 * sa + dr as u16 * inv) / 255) as u8;
                    let bg = ((sg as u16 * sa + dg as u16 * inv) / 255) as u8;
                    let bb = ((sb as u16 * sa + db as u16 * inv) / 255) as u8;
                    let px = ((br as u16 >> 3) << 11) | ((bg as u16 >> 2) << 5) | (bb as u16 >> 3);
                    dst_buf[di] = (px >> 8) as u8;
                    dst_buf[di + 1] = px as u8;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_565sw_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 2;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 2;
            let hi = src_buf[si];
            let lo = src_buf[si + 1];
            let d00 = dst_row0 + col * 4;
            let d10 = dst_row1 + col * 4;
            dst_buf[d00] = hi;
            dst_buf[d00 + 1] = lo;
            dst_buf[d00 + 2] = hi;
            dst_buf[d00 + 3] = lo;
            dst_buf[d10] = hi;
            dst_buf[d10 + 1] = lo;
            dst_buf[d10 + 2] = hi;
            dst_buf[d10 + 3] = lo;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_565(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            if a == 255 {
                let r = src_buf[si] as u16;
                let g = src_buf[si + 1] as u16;
                let b = src_buf[si + 2] as u16;
                let px = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                let lo = px as u8;
                let hi = (px >> 8) as u8;
                let d00 = dst_row0 + col * 4;
                let d10 = dst_row1 + col * 4;
                dst_buf[d00] = lo;
                dst_buf[d00 + 1] = hi;
                dst_buf[d00 + 2] = lo;
                dst_buf[d00 + 3] = hi;
                dst_buf[d10] = lo;
                dst_buf[d10 + 1] = hi;
                dst_buf[d10 + 2] = lo;
                dst_buf[d10 + 3] = hi;
            } else {
                let sr = src_buf[si];
                let sg = src_buf[si + 1];
                let sb = src_buf[si + 2];
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 4,
                    dst_row0 + col * 4 + 2,
                    dst_row1 + col * 4,
                    dst_row1 + col * 4 + 2,
                ] {
                    let lo = dst_buf[di] as u16;
                    let hi = dst_buf[di + 1] as u16;
                    let dpx = lo | (hi << 8);
                    let dr = ((dpx >> 11) as u8) << 3;
                    let dg = (((dpx >> 5) & 0x3F) as u8) << 2;
                    let db = ((dpx & 0x1F) as u8) << 3;
                    let br = ((sr as u16 * sa + dr as u16 * inv) / 255) as u8;
                    let bg = ((sg as u16 * sa + dg as u16 * inv) / 255) as u8;
                    let bb = ((sb as u16 * sa + db as u16 * inv) / 255) as u8;
                    let px = ((br as u16 >> 3) << 11) | ((bg as u16 >> 2) << 5) | (bb as u16 >> 3);
                    dst_buf[di] = px as u8;
                    dst_buf[di + 1] = (px >> 8) as u8;
                }
            }
        }
    }
}

/// DDA (digital differential analyzer) blit: one divide per axis at
/// the top of the function, then each row/column just adds the step.
/// Step is stored in Q16.16 so the high word lands on an integer src
/// sample index and the low word carries the fractional error across
/// iterations — identical sampling result to `(drow * sh) / dh` but
/// without RV32's software divide in the inner loop.
#[allow(clippy::too_many_arguments)]
fn blit_dda(
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
    // Step values in Q16.16 fixed-point. `(sw << 16) / dw` lands the
    // integer portion in the high 16 bits; adding step each iteration
    // is exact modular arithmetic on a u32.
    let sx_step = ((sw as u32) << 16) / dw as u32;
    let sy_step = ((sh as u32) << 16) / dh as u32;

    let mut sy_acc: u32 = 0;
    for drow in 0..dh {
        let iy = dy0 + drow;
        if iy < clip_y0 || iy >= clip_y1 {
            sy_acc = sy_acc.wrapping_add(sy_step);
            continue;
        }
        let sy = sy0 + (sy_acc >> 16) as i32;
        sy_acc = sy_acc.wrapping_add(sy_step);

        let mut sx_acc: u32 = 0;
        for dcol in 0..dw {
            let ix = dx0 + dcol;
            if ix < clip_x0 || ix >= clip_x1 {
                sx_acc = sx_acc.wrapping_add(sx_step);
                continue;
            }
            let sx = sx0 + (sx_acc >> 16) as i32;
            sx_acc = sx_acc.wrapping_add(sx_step);

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
        use crate::types::TransformClass;

        if let DrawCommand::Fill {
            area,
            quad: Some(q),
            color,
            opa,
            radius,
            ..
        } = cmd
        {
            #[cfg(feature = "perf")]
            let t0 = quad_perf::now();
            let phys_clip = self.viewport.rect_to_physical(*clip);
            let phys_q = [
                self.viewport.point_to_physical(q[0]),
                self.viewport.point_to_physical(q[1]),
                self.viewport.point_to_physical(q[2]),
                self.viewport.point_to_physical(q[3]),
            ];
            let phys_w = area.w * self.viewport.scale();
            let phys_h = area.h * self.viewport.scale();
            let phys_radius = *radius * self.viewport.scale();
            fill_rect_quad(
                &mut self.target,
                &phys_q,
                phys_clip,
                color,
                phys_radius,
                phys_w,
                phys_h,
                *opa,
            );
            #[cfg(feature = "perf")]
            quad_perf::add_fill(quad_perf::now().wrapping_sub(t0));
            return;
        }
        if let DrawCommand::Blit {
            quad: Some(q),
            texture,
            ..
        } = cmd
        {
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
            return;
        }

        let tf = cmd.transform();
        let class = tf.classify();
        if !matches!(class, TransformClass::Identity | TransformClass::Translate) {
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
                #[cfg(feature = "perf")]
                let t0 = self.perf.as_ref().map(|p| (p.clock)());
                let area = offset_rect(area, tx, ty);
                self.fill_rect(&area, clip, color, *radius, *opa);
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
                let area = offset_rect(area, tx, ty);
                self.stroke_rect(&area, clip, *width, color, *radius, *opa);
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
                let pos = offset_point(pos, tx, ty);
                self.blit(texture, &src_rect, pos, *size, clip);
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
                let pos = offset_point(pos, tx, ty);
                self.draw_label(&pos, text, clip, color, *opa);
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
                let p1 = offset_point(p1, tx, ty);
                let p2 = offset_point(p2, tx, ty);
                self.draw_line(p1, p2, clip, *width, color, *opa);
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
                let center = offset_point(center, tx, ty);
                self.draw_arc(
                    center,
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
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::ARGB8888);

        // Dst: 8×6, draw 7×5 blit at (0,0). Two identical dst buffers.
        let mut dst_a = vec![0u8; 8 * 6 * 4];
        let mut dst_b = vec![0u8; 8 * 6 * 4];

        {
            let mut tex_a = Texture::new(&mut dst_a, 8, 6, ColorFormat::ARGB8888);
            blit_generic_slow(&mut tex_a, &src, 0, 0, 4, 4, 0, 0, 7, 5, 0, 0, 8, 6);
        }
        {
            let mut tex_b = Texture::new(&mut dst_b, 8, 6, ColorFormat::ARGB8888);
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
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::ARGB8888);

        let mut dst_a = vec![0u8; 6 * 6 * 4];
        for (i, byte) in dst_a.iter_mut().enumerate() {
            *byte = (i * 3) as u8;
        }
        let mut dst_b = dst_a.clone();

        {
            let mut tex = Texture::new(&mut dst_a, 6, 6, ColorFormat::ARGB8888);
            blit_generic_slow(&mut tex, &src, 0, 0, 4, 4, 1, 1, 4, 4, 0, 0, 6, 6);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 6, 6, ColorFormat::ARGB8888);
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
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::ARGB8888);

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
        let mut dst = Texture::new(&mut buf, 16, 16, ColorFormat::ARGB8888);
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
    fn blit_1to1_with_clip_restricted() {
        let mut src_buf = vec![0u8; 4 * 4 * 4];
        for i in 0..16 {
            src_buf[i * 4] = 100 + i as u8;
            src_buf[i * 4 + 3] = 255;
        }
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::ARGB8888);

        let mut dst_a = vec![0u8; 8 * 8 * 4];
        let mut dst_b = vec![0u8; 8 * 8 * 4];

        // Clip covers only the right half of the blit rect.
        {
            let mut tex = Texture::new(&mut dst_a, 8, 8, ColorFormat::ARGB8888);
            blit_generic_slow(&mut tex, &src, 0, 0, 4, 4, 1, 1, 4, 4, 3, 0, 8, 8);
        }
        {
            let mut tex = Texture::new(&mut dst_b, 8, 8, ColorFormat::ARGB8888);
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
        let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::ARGB8888);

        let mut dst_a = vec![0u8; 10 * 10 * 4];
        let mut dst_b = vec![0u8; 10 * 10 * 4];
        // Clip covers columns 2..7 only.
        {
            let mut tex_a = Texture::new(&mut dst_a, 10, 10, ColorFormat::ARGB8888);
            blit_generic_slow(&mut tex_a, &src, 0, 0, 4, 4, 1, 1, 8, 8, 2, 0, 7, 10);
        }
        {
            let mut tex_b = Texture::new(&mut dst_b, 10, 10, ColorFormat::ARGB8888);
            blit_dda(&mut tex_b, &src, 0, 0, 4, 4, 1, 1, 8, 8, 2, 0, 7, 10);
        }
        assert_eq!(dst_a, dst_b);
    }
}
