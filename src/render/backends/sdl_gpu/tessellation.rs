//! Converts shared tessellation output into SDL vertices and indices.
//!
//! Tessellators and vertex/index buffers retain capacity between frames.

use alloc::vec::Vec;

use lyon::math::Point as LyonPoint;
use lyon::tessellation::VertexBuffers;
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::render::backends::tessellation::{
    FillTessellator, StrokeTessellator as SharedStrokeTessellator,
};
use crate::render::path::Path;
use crate::render::raster::{FillRule, StrokeSpec};
use crate::render::renderer::RenderError;
use crate::types::{Color, Transform};

/// Reusable tessellators and output buffers owned by the renderer factory.
pub struct TessellationCache {
    fill_tess: FillTessellator,
    stroke_tess: SharedStrokeTessellator,
    pub(super) verts: Vec<SDL_Vertex>,
    pub(super) indices: Vec<i32>,
}

impl TessellationCache {
    pub fn new() -> Self {
        Self {
            fill_tess: FillTessellator::new(),
            stroke_tess: SharedStrokeTessellator::new(),
            verts: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn fill(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) -> Result<(), RenderError> {
        self.verts.clear();
        self.indices.clear();
        self.fill_tess.fill(path, transform, fill_rule)?;
        let mesh = self.fill_tess.take_mesh();
        self.bake(&mesh, color, opa);
        self.fill_tess.restore_mesh(mesh);
        Ok(())
    }

    pub fn stroke(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
        color: &Color,
        opa: u8,
    ) -> Result<(), RenderError> {
        self.verts.clear();
        self.indices.clear();
        self.stroke_tess.stroke(path, transform, spec)?;
        let mesh = self.stroke_tess.take_mesh();
        self.bake(&mesh, color, opa);
        self.stroke_tess.restore_mesh(mesh);
        Ok(())
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
