//! Converts shared tessellation output into SDL vertices and indices.
//!
//! Tessellators and vertex/index buffers retain capacity between frames.

use alloc::vec::Vec;

use lyon::math::Point as LyonPoint;
use lyon::tessellation::{
    BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::render::backends::lyon_path::to_lyon_path;
use crate::render::backends::tessellation::FillTessellator;
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
    stroke_buffers: VertexBuffers<LyonPoint, u32>,
}

impl TessellationCache {
    pub fn new() -> Self {
        Self {
            fill_tess: FillTessellator::new(),
            stroke_tess: StrokeTessellator::new(),
            verts: Vec::new(),
            indices: Vec::new(),
            stroke_buffers: VertexBuffers::new(),
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
        self.fill_tess.fill(path, transform, fill_rule);
        let mesh = self.fill_tess.take_mesh();
        self.bake(&mesh, color, opa);
        self.fill_tess.restore_mesh(mesh);
    }

    pub fn stroke(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        physical_width: f32,
        color: &Color,
        opa: u8,
    ) {
        self.stroke_buffers.vertices.clear();
        self.stroke_buffers.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let options = StrokeOptions::tolerance(0.5).with_line_width(physical_width);
        let _ = self.stroke_tess.tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut self.stroke_buffers, |v: StrokeVertex<'_, '_>| {
                v.position()
            }),
        );
        let mesh = core::mem::replace(&mut self.stroke_buffers, VertexBuffers::new());
        self.bake(&mesh, color, opa);
        self.stroke_buffers = mesh;
    }

    fn bake(&mut self, mesh: &VertexBuffers<LyonPoint, u32>, color: &Color, opa: u8) {
        let a = ((color.a as u16) * (opa as u16) / 255) as u8;
        let sdl_color = SDL_Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a,
        };
        self.verts.clear();
        self.verts.reserve(mesh.vertices.len());
        for p in &mesh.vertices {
            self.verts.push(SDL_Vertex {
                position: SDL_FPoint { x: p.x, y: p.y },
                color: sdl_color,
                tex_coord: SDL_FPoint { x: 0.0, y: 0.0 },
            });
        }
        self.indices.clear();
        self.indices.reserve(mesh.indices.len());
        for i in &mesh.indices {
            self.indices.push(*i as i32);
        }
    }
}

impl Default for TessellationCache {
    fn default() -> Self {
        Self::new()
    }
}
