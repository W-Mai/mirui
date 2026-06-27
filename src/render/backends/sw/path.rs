use super::SwRenderer;
use crate::render::path::{self, Path};
use crate::render::raster;
use crate::types::{Color, Fixed, Rect, Transform};

impl SwRenderer<'_> {
    pub(super) fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.fill_path_transformed(path, phys_clip, &phys_tf, color, opa);
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
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &self.flatten_buf;
        let target = &mut self.target;
        let acc = &mut self.scanline_acc;
        let crossings = &mut self.scanline_crossings;
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            raster::FillRule::EvenOdd,
            acc,
            crossings,
            |px, py, cov| {
                let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                if final_alpha > 0 {
                    target.blend_pixel_int(px, py, color, final_alpha);
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
        let phys_tf = self.viewport.as_transform();
        let phys_width = width * self.viewport.scale();
        raster::offset_polygon_into(
            &path.cmds,
            Some(&phys_tf),
            phys_width,
            &mut self.stroke_outline,
            &mut self.subpath_scratch,
        );
        // outline is already in physical pixels.
        let outline_cmds = core::mem::take(&mut self.stroke_outline);
        self.fill_physical_path(&outline_cmds, clip, color, opa);
        self.stroke_outline = outline_cmds;
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

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let color_a_norm =
            Fixed::from_int(color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &self.flatten_buf;
        let target = &mut self.target;
        let acc = &mut self.scanline_acc;
        let crossings = &mut self.scanline_crossings;
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            raster::FillRule::EvenOdd,
            acc,
            crossings,
            |px, py, cov| {
                let final_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                if final_alpha > 0 {
                    target.blend_pixel_int(px, py, color, final_alpha);
                }
            },
        );
    }
}
