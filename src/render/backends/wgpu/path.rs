//! lyon tessellation bridge for wgpu path draws.

use lyon::math::Point as LyonPoint;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};

use crate::render::backends::lyon_path::to_lyon_path;
use crate::render::path::Path;
use crate::types::Transform;

pub struct PathTessellator {
    fill_tess: FillTessellator,
    buffers: VertexBuffers<LyonPoint, u32>,
}

impl PathTessellator {
    pub fn new() -> Self {
        Self {
            fill_tess: FillTessellator::new(),
            buffers: VertexBuffers::new(),
        }
    }

    pub fn fill(&mut self, path: &Path, transform: Option<&Transform>) -> (&[LyonPoint], &[u32]) {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let _ = self.fill_tess.tessellate_path(
            &lyon_path,
            &FillOptions::tolerance(TOLERANCE),
            &mut BuffersBuilder::new(&mut self.buffers, |v: FillVertex<'_>| v.position()),
        );
        (&self.buffers.vertices, &self.buffers.indices)
    }

    pub fn take_mesh(&mut self) -> VertexBuffers<LyonPoint, u32> {
        core::mem::replace(&mut self.buffers, VertexBuffers::new())
    }

    pub fn restore_mesh(&mut self, mesh: VertexBuffers<LyonPoint, u32>) {
        self.buffers = mesh;
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
