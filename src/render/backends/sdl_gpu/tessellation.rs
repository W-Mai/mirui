//! Bridge mirui's fixed-point [`Path`](crate::render::path::Path) to lyon
//! tessellation output that can be fed to SDL_RenderGeometry. All the
//! lyon-specific machinery lives here so the rest of the GPU backend
//! only depends on raw `SDL_Vertex` / index pairs.
//!
//! Every call reuses the FillTessellator / StrokeTessellator instances
//! and the vertex / index buffers: tessellating a complex path allocates
//! once on first use and then amortises to zero.

use alloc::vec::Vec;

use lyon::math::Point as LyonPoint;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::render::backends::lyon_path::to_lyon_path;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::types::{Color, Transform};

/// Holds reusable tessellators and output buffers so path commands don't
/// re-allocate every frame. One cache per `SdlGpuSurface`.
pub struct TessellationCache {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
    pub(super) verts: Vec<SDL_Vertex>,
    pub(super) indices: Vec<i32>,
    lyon_verts: VertexBuffers<LyonPoint, u32>,
}

impl TessellationCache {
    pub fn new() -> Self {
        Self {
            fill_tess: FillTessellator::new(),
            stroke_tess: StrokeTessellator::new(),
            verts: Vec::new(),
            indices: Vec::new(),
            lyon_verts: VertexBuffers::new(),
        }
    }

    pub fn fill(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) {
        self.lyon_verts.vertices.clear();
        self.lyon_verts.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let _ = self.fill_tess.tessellate_path(
            &lyon_path,
            &FillOptions::tolerance(1.0).with_fill_rule(match fill_rule {
                FillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                FillRule::NonZero => lyon::tessellation::FillRule::NonZero,
            }),
            &mut BuffersBuilder::new(&mut self.lyon_verts, |v: FillVertex<'_>| v.position()),
        );
        self.bake(color, opa);
    }

    pub fn stroke(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        physical_width: f32,
        color: &Color,
        opa: u8,
    ) {
        self.lyon_verts.vertices.clear();
        self.lyon_verts.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let options = StrokeOptions::tolerance(0.5).with_line_width(physical_width);
        let _ = self.stroke_tess.tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut self.lyon_verts, |v: StrokeVertex<'_, '_>| v.position()),
        );
        self.bake(color, opa);
    }

    fn bake(&mut self, color: &Color, opa: u8) {
        let a = ((color.a as u16) * (opa as u16) / 255) as u8;
        let sdl_color = SDL_Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a,
        };
        self.verts.clear();
        self.verts.reserve(self.lyon_verts.vertices.len());
        for p in &self.lyon_verts.vertices {
            self.verts.push(SDL_Vertex {
                position: SDL_FPoint { x: p.x, y: p.y },
                color: sdl_color,
                tex_coord: SDL_FPoint { x: 0.0, y: 0.0 },
            });
        }
        self.indices.clear();
        self.indices.reserve(self.lyon_verts.indices.len());
        for i in &self.lyon_verts.indices {
            self.indices.push(*i as i32);
        }
    }
}

impl Default for TessellationCache {
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
        let mut tessellator = TessellationCache::new();
        let mut area = |rule| {
            tessellator.fill(&path, None, &Color::rgb(255, 255, 255), 255, rule);
            let vertices = &tessellator.lyon_verts.vertices;
            tessellator
                .lyon_verts
                .indices
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
