use super::SdlGpuRenderer;
use crate::render::path::Path;
use crate::render::raster::{FillRule, LineCap, LineJoin, StrokeSpec};
use crate::types::{Color, Fixed, Rect, Transform};

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn fill_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) {
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator
            .fill(path, Some(&phys_tf), color, opa, fill_rule);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn fill_path_transformed_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        cmd_tf: &Transform,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) {
        let phys_tf = self.viewport.as_transform().compose(cmd_tf);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator
            .fill(path, Some(&phys_tf), color, opa, fill_rule);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn stroke_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        self.stroke_path_styled_inner(
            path,
            clip,
            &Transform::IDENTITY,
            StrokeSpec {
                width,
                cap: LineCap::Butt,
                join: LineJoin::Miter,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
            color,
            opa,
        );
    }

    pub(super) fn stroke_path_styled_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        cmd_tf: &Transform,
        spec: StrokeSpec<'_>,
        color: &Color,
        opa: u8,
    ) {
        if spec.width <= Fixed::ZERO || opa == 0 {
            return;
        }
        let scale = self.viewport.scale();
        let phys_tf = self.viewport.as_transform().compose(cmd_tf);
        let outline = self.stroke_scratch.outline(
            path,
            Some(&phys_tf),
            StrokeSpec {
                width: spec.width * scale,
                dash_scale: scale,
                ..spec
            },
        );
        self.tessellator
            .fill(outline, None, color, opa, FillRule::NonZero);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }
}
