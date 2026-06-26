use super::SdlGpuRenderer;
use crate::render::path::Path;
use crate::types::{Color, Fixed, Rect, Transform};

impl SdlGpuRenderer<'_> {
    pub(super) fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator.fill(path, Some(&phys_tf), color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn fill_path_transformed_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        cmd_tf: &Transform,
        color: &Color,
        opa: u8,
    ) {
        let phys_tf = self.viewport.as_transform().compose(cmd_tf);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator.fill(path, Some(&phys_tf), color, opa);
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
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let physical_width = (width * self.viewport.scale()).to_f32().max(1.0);
        self.tessellator
            .stroke(path, Some(&phys_tf), physical_width, color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }
}
