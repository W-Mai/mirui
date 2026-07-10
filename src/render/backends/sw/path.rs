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

impl SwRenderer<'_> {
    pub(super) fn push_clip_inner(
        &mut self,
        path: &Path,
        phys_tf: &Transform,
        fill_rule: FillRule,
    ) {
        let w = self.target.width as usize;
        let h = self.target.height as usize;
        self.clip_mask_buf.clear();
        self.clip_mask_buf.resize(w * h, 0);

        raster::flatten_into(&path.cmds, Some(phys_tf), &mut self.flatten_buf);
        if !self.flatten_buf.is_empty() {
            let screen = Rect::new(0, 0, self.target.width, self.target.height);
            if let Some(bbox) = path::bbox_of_cmds_transformed(&path.cmds, Some(phys_tf)) {
                if let Some(draw_area) = bbox.intersect(&screen) {
                    let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
                    let segs = &self.flatten_buf;
                    let acc = &mut self.scanline_acc;
                    let crossings = &mut self.scanline_crossings;
                    let mask = &mut self.clip_mask_buf;
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

        if let Some(prev) = self.clip_stack.last() {
            for (dst, prev) in self.clip_mask_buf.iter_mut().zip(prev.alpha.iter()) {
                *dst = (*dst).min(*prev);
            }
        }

        let alpha = core::mem::take(&mut self.clip_mask_buf);
        self.clip_stack.push(super::ClipMask { alpha });
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
        raster::flatten_into(&path.cmds, Some(phys_tf), &mut self.flatten_buf);
        if self.flatten_buf.is_empty() {
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
        let color = paint_color(paint);
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &self.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut self.scanline_acc;
        let crossings = &mut self.scanline_crossings;
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
                    target.blend_pixel_int(px, py, &color, final_alpha);
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
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_width = width * self.viewport.scale();
        raster::offset_polygon_into(
            &path.cmds,
            Some(&phys_tf),
            phys_width,
            cap,
            join,
            miter_limit,
            &mut self.stroke_outline,
            &mut self.subpath_scratch,
            &mut self.stroke_normals,
            &mut self.stroke_rail,
            &mut self.stroke_arc,
        );
        let outline_cmds = core::mem::take(&mut self.stroke_outline);
        self.fill_physical_path_with_paint(&outline_cmds, clip, paint, opa);
        self.stroke_outline = outline_cmds;
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
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_width = width * self.viewport.scale();
        raster::offset_polygon_into(
            &path.cmds,
            Some(phys_tf),
            phys_width,
            cap,
            join,
            miter_limit,
            &mut self.stroke_outline,
            &mut self.subpath_scratch,
            &mut self.stroke_normals,
            &mut self.stroke_rail,
            &mut self.stroke_arc,
        );
        let outline_cmds = core::mem::take(&mut self.stroke_outline);
        self.fill_physical_path_with_paint(&outline_cmds, &phys_clip, paint, opa);
        self.stroke_outline = outline_cmds;
    }

    pub(super) fn fill_physical_path_with_paint(
        &mut self,
        phys_path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
    ) {
        let color = paint_color(paint);
        self.fill_physical_path(phys_path, clip, &color, opa);
    }

    pub(super) fn fill_physical_path(
        &mut self,
        phys_path: &Path,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        if opa == 0 {
            return;
        }
        let phys_clip = self.viewport.rect_to_physical(*clip);
        raster::flatten_into(&phys_path.cmds, None, &mut self.flatten_buf);
        if self.flatten_buf.is_empty() {
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
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &self.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut self.scanline_acc;
        let crossings = &mut self.scanline_crossings;
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
                    target.blend_pixel_int(px, py, color, final_alpha);
                }
            },
        );
    }
}
