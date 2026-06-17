use super::SdlGpuRenderer;
use crate::render::path::{Path, PathCmd};
use crate::types::{Color, Fixed, Point, Rect};

impl SdlGpuRenderer<'_> {
    /// Scale every Point inside `path` into physical pixels to feed the
    /// tessellator (which produces physical vertex positions).
    pub(super) fn scale_path(&self, path: &Path) -> Path {
        let s = self.viewport.scale();
        let cmds = path
            .cmds
            .iter()
            .map(|c| match c {
                PathCmd::MoveTo(p) => PathCmd::MoveTo(Point {
                    x: p.x * s,
                    y: p.y * s,
                }),
                PathCmd::LineTo(p) => PathCmd::LineTo(Point {
                    x: p.x * s,
                    y: p.y * s,
                }),
                PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                    ctrl: Point {
                        x: ctrl.x * s,
                        y: ctrl.y * s,
                    },
                    end: Point {
                        x: end.x * s,
                        y: end.y * s,
                    },
                },
                PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
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
                },
                PathCmd::Close => PathCmd::Close,
            })
            .collect();
        Path { cmds }
    }

    pub(super) fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        let phys_path = self.scale_path(path);
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
