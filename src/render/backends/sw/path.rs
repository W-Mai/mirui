use super::SwRenderer;
use crate::render::canvas::Paint;
use crate::render::path::{self, Path};
use crate::render::raster::{self, FillRule};
use crate::types::{Color, Fixed, Rect, Transform};

fn paint_color(paint: &Paint) -> Color {
    match paint {
        Paint::Color(color) => (*color).into(),
        Paint::LinearGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
        Paint::RadialGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
    }
}

fn map_units(c: mirx::types::Point, units: mirx::scene::GradientUnits, bbox: Rect) -> Fixed {
    let cx: Fixed = c.x.into();
    match units {
        mirx::scene::GradientUnits::UserSpaceOnUse => cx,
        mirx::scene::GradientUnits::ObjectBoundingBox => bbox.x + cx * bbox.w,
    }
}

fn map_units_y(c: mirx::types::Point, units: mirx::scene::GradientUnits, bbox: Rect) -> Fixed {
    let cy: Fixed = c.y.into();
    match units {
        mirx::scene::GradientUnits::UserSpaceOnUse => cy,
        mirx::scene::GradientUnits::ObjectBoundingBox => bbox.y + cy * bbox.h,
    }
}

fn map_scalar(
    v: mirx::types::Fixed,
    units: mirx::scene::GradientUnits,
    bbox: Rect,
    axis_w: bool,
) -> Fixed {
    let v: Fixed = v.into();
    match units {
        mirx::scene::GradientUnits::UserSpaceOnUse => v,
        mirx::scene::GradientUnits::ObjectBoundingBox => {
            if axis_w {
                v * bbox.w
            } else {
                v * bbox.h
            }
        }
    }
}

fn apply_spread(t: Fixed, spread: mirx::scene::SpreadMode) -> Fixed {
    match spread {
        mirx::scene::SpreadMode::Pad => t,
        mirx::scene::SpreadMode::Repeat => {
            if t < Fixed::ZERO {
                let mut r = t;
                while r < Fixed::ZERO {
                    r += Fixed::ONE;
                }
                r
            } else {
                t - t.floor()
            }
        }
        mirx::scene::SpreadMode::Reflect => {
            let mut r = t;
            if r < Fixed::ZERO {
                r = -r;
            }
            let floor = r.floor();
            let frac = r - floor;
            let period = floor.to_int() % 2;
            if period == 0 { frac } else { Fixed::ONE - frac }
        }
    }
}

fn sample_gradient(paint: &Paint, px: i32, py: i32, bbox: Rect) -> Color {
    match paint {
        Paint::Color(c) => (*c).into(),
        Paint::LinearGradient(g) => {
            let sx = map_units(g.start, g.units, bbox);
            let sy = map_units_y(g.start, g.units, bbox);
            let ex = map_units(g.end, g.units, bbox);
            let ey = map_units_y(g.end, g.units, bbox);
            let x = Fixed::from_int(px);
            let y = Fixed::from_int(py);
            let dx = ex - sx;
            let dy = ey - sy;
            let len_sq = dx * dx + dy * dy;
            if len_sq == Fixed::ZERO {
                return g
                    .stops
                    .first()
                    .map(|s| s.color.into())
                    .unwrap_or(Color::rgba(0, 0, 0, 0));
            }
            let t = ((x - sx) * dx + (y - sy) * dy) / len_sq;
            let t = apply_spread(t, g.spread);
            sample_stops(&g.stops, t)
        }
        Paint::RadialGradient(g) => {
            let cx = map_units(g.center, g.units, bbox);
            let cy = map_units_y(g.center, g.units, bbox);
            let r = map_scalar(g.radius, g.units, bbox, true);
            let x = Fixed::from_int(px);
            let y = Fixed::from_int(py);
            let dx = x - cx;
            let dy = y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if r == Fixed::ZERO {
                return g
                    .stops
                    .first()
                    .map(|s| s.color.into())
                    .unwrap_or(Color::rgba(0, 0, 0, 0));
            }
            let t = dist / r;
            let t = apply_spread(t, g.spread);
            sample_stops(&g.stops, t)
        }
    }
}

fn sample_stops(stops: &[mirx::scene::GradientStop], t: Fixed) -> Color {
    if stops.is_empty() {
        return Color::rgba(0, 0, 0, 0);
    }
    let t_raw: Fixed = t;
    let first_off: Fixed = stops[0].offset.into();
    let last_off: Fixed = stops[stops.len() - 1].offset.into();
    if t_raw <= first_off {
        return stops[0].color.into();
    }
    if t_raw >= last_off {
        return stops[stops.len() - 1].color.into();
    }
    for i in 0..stops.len() - 1 {
        let s0 = &stops[i];
        let s1 = &stops[i + 1];
        let o0: Fixed = s0.offset.into();
        let o1: Fixed = s1.offset.into();
        if t_raw >= o0 && t_raw <= o1 {
            let range = o1 - o0;
            if range == Fixed::ZERO {
                return s1.color.into();
            }
            let local_t = (t_raw - o0) / range;
            let lt = local_t.to_f32();
            let r = (s0.color.r as f32 + (s1.color.r as f32 - s0.color.r as f32) * lt) as u8;
            let g = (s0.color.g as f32 + (s1.color.g as f32 - s0.color.g as f32) * lt) as u8;
            let b = (s0.color.b as f32 + (s1.color.b as f32 - s0.color.b as f32) * lt) as u8;
            let a = (s0.color.a as f32 + (s1.color.a as f32 - s0.color.a as f32) * lt) as u8;
            return Color::rgba(r, g, b, a);
        }
    }
    stops[stops.len() - 1].color.into()
}

impl SwRenderer<'_> {
    pub(super) fn push_clip_inner(
        &mut self,
        path: &Path,
        phys_tf: &Transform,
        fill_rule: FillRule,
    ) {
        let w = self.target.width as usize;
        let h = self.target.height as usize;
        let scratch = &mut *self.scratch;
        if scratch.clip_mask_buf.capacity() < w * h {
            if let Some(index) = scratch
                .clip_recycled
                .iter()
                .position(|mask| mask.alpha.capacity() >= w * h)
            {
                let mut mask = scratch.clip_recycled.swap_remove(index);
                core::mem::swap(&mut scratch.clip_mask_buf, &mut mask.alpha);
                if mask.alpha.capacity() > 0 {
                    scratch.clip_recycled.push(mask);
                }
            }
        }
        scratch.clip_mask_buf.clear();
        scratch.clip_mask_buf.resize(w * h, 0);

        raster::flatten_into(&path.cmds, Some(phys_tf), &mut scratch.flatten_buf);
        if !scratch.flatten_buf.is_empty() {
            let screen = Rect::new(0, 0, self.target.width, self.target.height);
            if let Some(bbox) = path::bbox_of_cmds_transformed(&path.cmds, Some(phys_tf)) {
                if let Some(draw_area) = bbox.intersect(&screen) {
                    let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
                    let segs = &scratch.flatten_buf;
                    let acc = &mut scratch.scanline_acc;
                    let crossings = &mut scratch.scanline_crossings;
                    let mask = &mut scratch.clip_mask_buf;
                    raster::scanline_fill(
                        segs,
                        px_x0,
                        px_y0,
                        px_x1,
                        py_y1,
                        fill_rule,
                        acc,
                        crossings,
                        |px, py, cov| {
                            let idx = py as usize * w + px as usize;
                            mask[idx] = cov.map01(255).to_int() as u8;
                        },
                    );
                }
            }
        }

        if let Some(prev) = scratch.clip_stack.last() {
            for (dst, prev) in scratch.clip_mask_buf.iter_mut().zip(prev.alpha.iter()) {
                *dst = (*dst).min(*prev);
            }
        }

        let alpha = core::mem::take(&mut scratch.clip_mask_buf);
        scratch.clip_stack.push(super::ClipMask { alpha });
    }

    pub(super) fn fill_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) {
        if opa == 0 {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.fill_path_transformed(path, phys_clip, &phys_tf, paint, opa, fill_rule);
    }

    pub(super) fn fill_path_transformed(
        &mut self,
        path: &Path,
        phys_clip: Rect,
        phys_tf: &Transform,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) {
        if opa == 0 {
            return;
        }
        let scratch = &mut *self.scratch;
        raster::flatten_into(&path.cmds, Some(phys_tf), &mut scratch.flatten_buf);
        if scratch.flatten_buf.is_empty() {
            return;
        }
        // PathCmd bbox keeps the AA edge pixels at curve extrema that
        // a tight LineSeg hull would clip.
        let Some(bbox) = path::bbox_of_cmds_transformed(&path.cmds, Some(phys_tf)) else {
            return;
        };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bbox
            .intersect(&phys_clip)
            .and_then(|r| r.intersect(&screen))
        else {
            return;
        };

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let is_gradient = !matches!(paint, Paint::Color(_));
        let solid_color = if is_gradient {
            Color::rgba(255, 255, 255, 255)
        } else {
            paint_color(paint)
        };
        let color_a_norm =
            Fixed::from_int(solid_color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
        let paint_ref = paint;
        let grad_bbox = bbox;
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            fill_rule,
            acc,
            crossings,
            |px, py, cov| {
                let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                let clip_alpha = clip_mask
                    .map(|m| m[py as usize * target_w + px as usize])
                    .unwrap_or(255);
                let final_alpha = ((base_alpha as u16 * clip_alpha as u16 + 127) / 255) as u8;
                if final_alpha > 0 {
                    if is_gradient {
                        let c = sample_gradient(paint_ref, px, py, grad_bbox);
                        target.blend_pixel_int(px, py, &c, final_alpha);
                    } else {
                        target.blend_pixel_int(px, py, &solid_color, final_alpha);
                    }
                }
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_width = width * self.viewport.scale();
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                &path.cmds,
                Some(&phys_tf),
                phys_width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
            );
        }
        let outline_cmds = core::mem::take(&mut self.scratch.stroke_outline);
        self.fill_physical_path_with_paint(&outline_cmds, clip, paint, opa);
        self.scratch.stroke_outline = outline_cmds;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_path_transformed(
        &mut self,
        path: &Path,
        phys_clip: Rect,
        phys_tf: &Transform,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_width = width * self.viewport.scale();
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                &path.cmds,
                Some(phys_tf),
                phys_width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
            );
        }
        let outline_cmds = core::mem::take(&mut self.scratch.stroke_outline);
        self.fill_physical_path_with_paint(&outline_cmds, &phys_clip, paint, opa);
        self.scratch.stroke_outline = outline_cmds;
    }

    pub(super) fn fill_physical_path_with_paint(
        &mut self,
        phys_path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
    ) {
        let _ = paint_color(paint);
        self.fill_physical_path(phys_path, clip, paint, opa);
    }

    pub(super) fn fill_physical_path(
        &mut self,
        phys_path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
    ) {
        if opa == 0 {
            return;
        }
        let scratch = &mut *self.scratch;
        let phys_clip = self.viewport.rect_to_physical(*clip);
        raster::flatten_into(&phys_path.cmds, None, &mut scratch.flatten_buf);
        if scratch.flatten_buf.is_empty() {
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

        let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let is_gradient = !matches!(paint, Paint::Color(_));
        let solid_color = if is_gradient {
            Color::rgba(255, 255, 255, 255)
        } else {
            paint_color(paint)
        };
        let color_a_norm =
            Fixed::from_int(solid_color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
        let paint_ref = paint;
        let grad_bbox = bbox;
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            py_y1,
            FillRule::EvenOdd,
            acc,
            crossings,
            |px, py, cov| {
                let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                let clip_alpha = clip_mask
                    .map(|m| m[py as usize * target_w + px as usize])
                    .unwrap_or(255);
                let final_alpha = ((base_alpha as u16 * clip_alpha as u16 + 127) / 255) as u8;
                if final_alpha > 0 {
                    if is_gradient {
                        let c = sample_gradient(paint_ref, px, py, grad_bbox);
                        target.blend_pixel_int(px, py, &c, final_alpha);
                    } else {
                        target.blend_pixel_int(px, py, &solid_color, final_alpha);
                    }
                }
            },
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod gradient_tests {
    extern crate std;
    use super::*;
    use crate::prelude::Point;
    use crate::render::canvas::Canvas;
    use crate::render::path::Path;
    use crate::render::texture::{ColorFormat, Texture};
    use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};

    fn linear_paint_obb() -> Paint {
        Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point {
                x: Fixed::ZERO.into(),
                y: Fixed::ZERO.into(),
            },
            end: mirx::types::Point {
                x: Fixed::ONE.into(),
                y: Fixed::ONE.into(),
            },
            stops: std::borrow::Cow::Owned(std::vec![
                GradientStop {
                    offset: Fixed::ZERO.into(),
                    color: Color::rgb(50, 120, 255).into()
                },
                GradientStop {
                    offset: Fixed::ONE.into(),
                    color: Color::rgb(255, 70, 90).into()
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: Transform::IDENTITY.into(),
        })
    }

    #[test]
    fn linear_object_bounding_box_maps_to_path_bbox() {
        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, 64, 64);

        let mut path = Path::new();
        path.move_to(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        });
        path.line_to(Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(10),
        });
        path.line_to(Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(50),
        });
        path.line_to(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(50),
        });
        path.close();

        let paint = linear_paint_obb();
        backend.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);

        let tl = backend.target.get_pixel(12, 12);
        let br = backend.target.get_pixel(48, 48);
        std::eprintln!("tl={:?} br={:?}", tl, br);
        assert!(
            tl.b > tl.r,
            "top-left (near gradient start) should be blue-ish, got {:?}",
            tl
        );
        assert!(
            br.r > br.b,
            "bottom-right (near gradient end) should be red-ish, got {:?}",
            br
        );
    }
}
