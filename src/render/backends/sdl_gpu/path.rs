use super::SdlGpuRenderer;
use crate::render::path::{Path, PathCmd};
use crate::types::{Color, Fixed, Point, Rect, Transform};

impl SdlGpuRenderer<'_> {
    /// Apply `phys_tf` to every Point inside `path`. Default path uses
    /// `viewport.as_transform()` for the logical→physical scale only;
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
        let phys_path = self.scale_path(path);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator.fill(&phys_path, color, opa);
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
        let phys_path = self.scale_path_with_tf(path, &phys_tf);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator.fill(&phys_path, color, opa);
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
        let phys_path = self.scale_path(path);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let physical_width = (width * self.viewport.scale()).to_f32().max(1.0);
        self.tessellator
            .stroke(&phys_path, physical_width, color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }
}
