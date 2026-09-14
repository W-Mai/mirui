//! lyon tessellation bridge for wgpu path draws.

use lyon::math::Point as LyonPoint;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonFillRule, FillTessellator, FillVertex,
    VertexBuffers,
};

use crate::render::backends::lyon_path::to_lyon_path;
use crate::render::path::Path;
use crate::render::raster::FillRule;
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

    pub fn fill(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        fill_rule: FillRule,
    ) -> (&[LyonPoint], &[u32]) {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let _ = self.fill_tess.tessellate_path(
            &lyon_path,
            &FillOptions::tolerance(TOLERANCE).with_fill_rule(match fill_rule {
                FillRule::EvenOdd => LyonFillRule::EvenOdd,
                FillRule::NonZero => LyonFillRule::NonZero,
            }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Point;

    #[test]
    fn nested_contours_follow_requested_fill_rule() {
        let mut path = Path::new();
        for (lo, hi) in [(0, 10), (2, 8)] {
            path.move_to(Point::new(lo, lo))
                .line_to(Point::new(hi, lo))
                .line_to(Point::new(hi, hi))
                .line_to(Point::new(lo, hi))
                .close();
        }
        let mut tessellator = PathTessellator::new();
        let mut area = |rule| {
            let (vertices, indices) = tessellator.fill(&path, None, rule);
            indices
                .chunks_exact(3)
                .map(|triangle| {
                    let [a, b, c] = [
                        vertices[triangle[0] as usize],
                        vertices[triangle[1] as usize],
                        vertices[triangle[2] as usize],
                    ];
                    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0
                })
                .sum::<f32>()
        };
        assert!((area(FillRule::EvenOdd) - 64.0).abs() < 0.01);
        assert!((area(FillRule::NonZero) - 100.0).abs() < 0.01);
    }
}
