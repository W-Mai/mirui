//! lyon tessellation bridge for wgpu path draws.

use lyon::math::{Point as LyonPoint, point as lyon_point};
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};

use crate::render::path::{Path, PathCmd};

pub struct PathTessellator {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
    buffers: VertexBuffers<LyonPoint, u32>,
}

impl PathTessellator {
    pub fn new() -> Self {
        Self {
            fill_tess: FillTessellator::new(),
            stroke_tess: StrokeTessellator::new(),
            buffers: VertexBuffers::new(),
        }
    }

    pub fn fill(&mut self, path: &Path) -> (&[LyonPoint], &[u32]) {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let lyon_path = to_lyon_path(path);
        let _ = self.fill_tess.tessellate_path(
            &lyon_path,
            &FillOptions::tolerance(TOLERANCE),
            &mut BuffersBuilder::new(&mut self.buffers, |v: FillVertex<'_>| v.position()),
        );
        (&self.buffers.vertices, &self.buffers.indices)
    }

    pub fn stroke(&mut self, path: &Path, physical_width: f32) -> (&[LyonPoint], &[u32]) {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let lyon_path = to_lyon_path(path);
        let options = StrokeOptions::tolerance(TOLERANCE).with_line_width(physical_width);
        let _ = self.stroke_tess.tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut self.buffers, |v: StrokeVertex<'_, '_>| v.position()),
        );
        (&self.buffers.vertices, &self.buffers.indices)
    }
}

/// Curve flattening tolerance in physical pixels. 0.1 keeps small
/// (≤8 px) corners visibly smooth at 1× DPI; bumping it makes 8-radius
/// corners look hexagonal.
const TOLERANCE: f32 = 0.1;

impl Default for PathTessellator {
    fn default() -> Self {
        Self::new()
    }
}

fn to_lyon_path(path: &Path) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut subpath_open = false;

    let p = |pt: crate::types::Point| -> LyonPoint { lyon_point(pt.x.to_f32(), pt.y.to_f32()) };

    for cmd in path.cmds.iter() {
        match cmd {
            PathCmd::MoveTo(pt) => {
                if subpath_open {
                    builder.end(false);
                }
                builder.begin(p(*pt));
                subpath_open = true;
            }
            PathCmd::LineTo(pt) => {
                if !subpath_open {
                    builder.begin(p(*pt));
                    subpath_open = true;
                    continue;
                }
                builder.line_to(p(*pt));
            }
            PathCmd::QuadTo { ctrl, end } => {
                if !subpath_open {
                    builder.begin(p(*end));
                    subpath_open = true;
                    continue;
                }
                builder.quadratic_bezier_to(p(*ctrl), p(*end));
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                if !subpath_open {
                    builder.begin(p(*end));
                    subpath_open = true;
                    continue;
                }
                builder.cubic_bezier_to(p(*ctrl1), p(*ctrl2), p(*end));
            }
            PathCmd::Close => {
                if subpath_open {
                    builder.end(true);
                    subpath_open = false;
                }
            }
        }
    }
    if subpath_open {
        builder.end(false);
    }
    builder.build()
}
