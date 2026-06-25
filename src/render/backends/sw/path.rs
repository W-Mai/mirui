use super::SwRenderer;
use crate::render::path::{Path, PathCmd};
use crate::render::raster;
use crate::types::{Color, Fixed, Point, Rect, Transform};

impl SwRenderer<'_> {
    /// Apply `phys_tf` to every Point inside `path`. Default callers
    /// pass `viewport.as_transform()` (logical→physical scale);
    /// transformed callers compose viewport × cmd transform.
    pub(super) fn scale_path_with_tf(&self, path: &Path, phys_tf: &Transform) -> Path {
        let apply = |p: &Point| phys_tf.apply_point(*p);
        let cmds = path
            .cmds
            .iter()
            .map(|c| match c {
                PathCmd::MoveTo(p) => PathCmd::MoveTo(apply(p)),
                PathCmd::LineTo(p) => PathCmd::LineTo(apply(p)),
                PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                    ctrl: apply(ctrl),
                    end: apply(end),
                },
                PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
                    ctrl1: apply(ctrl1),
                    ctrl2: apply(ctrl2),
                    end: apply(end),
                },
                PathCmd::Close => PathCmd::Close,
            })
            .collect();
        Path { cmds }
    }

    pub(super) fn scale_path(&self, path: &Path) -> Path {
        self.scale_path_with_tf(path, &self.viewport.as_transform())
    }

    pub(super) fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }
        let phys_path = self.scale_path(path);
        self.fill_physical_path(&phys_path, clip, color, opa);
    }

    pub(super) fn fill_path_transformed(
        &mut self,
        path: &Path,
        phys_clip: Rect,
        phys_tf: &Transform,
        color: &Color,
        opa: u8,
    ) {
        if opa == 0 {
            return;
        }
        let phys_path = self.scale_path_with_tf(path, phys_tf);
        // phys_clip is already physical — skip the rect_to_physical call
        // that fill_physical_path does (its callers pass logical clip).
        let segs = raster::flatten(&phys_path);
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

        raster::scanline_fill(
            &segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            raster::FillRule::EvenOdd,
            |px, py, cov| {
                let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                if final_alpha > 0 {
                    self.target.blend_pixel_int(px, py, color, final_alpha);
                }
            },
        );
    }

    pub(super) fn stroke_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_path = self.scale_path(path);
        let phys_width = width * self.viewport.scale();
        let outline = raster::offset_polygon(&phys_path, phys_width);
        // `outline` is already in physical coords — skip the scale step.
        self.fill_physical_path(&outline, clip, color, opa);
    }

    /// Rasterize an already-physical-coord path; used by stroke_path to
    /// avoid re-scaling the offset outline it already produced.
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
        let segs = raster::flatten(phys_path);
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

        raster::scanline_fill(
            &segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            raster::FillRule::EvenOdd,
            |px, py, cov| {
                let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                if final_alpha > 0 {
                    self.target.blend_pixel_int(px, py, color, final_alpha);
                }
            },
        );
    }
}
