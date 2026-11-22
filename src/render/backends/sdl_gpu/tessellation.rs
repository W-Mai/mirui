//! Converts shared tessellation output into SDL vertices and indices.
//!
//! Tessellators and vertex/index buffers retain capacity between frames.

use alloc::vec::Vec;

use lyon::math::Point as LyonPoint;
use lyon::tessellation::VertexBuffers;
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::render::backends::lyon_path::commands_to_lyon;
use crate::render::backends::tessellation::geometry::{GeometryCache, ShapeRequest, ShapeStyle};
use crate::render::backends::tessellation::{
    FillTessellator, StrokeTessellator as SharedStrokeTessellator,
};
#[cfg(test)]
use crate::render::path::Path;
use crate::render::path::PathCmd;
use crate::render::raster::{FillRule, StrokeScratch, StrokeSpec};
use crate::render::renderer::RenderError;
use crate::types::{Color, Point, Transform};

/// Reusable tessellators and output buffers owned by the renderer factory.
pub struct TessellationCache {
    pub(super) geometry: GeometryCache,
    fill_tess: FillTessellator,
    stroke_tess: SharedStrokeTessellator,
    pub(super) verts: Vec<SDL_Vertex>,
    pub(super) indices: Vec<i32>,
    pub(super) gradient_batch: Vec<SDL_Vertex>,
}

impl TessellationCache {
    pub fn new() -> Self {
        Self {
            geometry: GeometryCache::new(1024 * 1024),
            fill_tess: FillTessellator::new(),
            stroke_tess: SharedStrokeTessellator::new(),
            verts: Vec::new(),
            indices: Vec::new(),
            gradient_batch: Vec::new(),
        }
    }

    pub fn fill(
        &mut self,
        path: &[PathCmd],
        transform: Option<&Transform>,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) -> Result<(), RenderError> {
        self.verts.clear();
        self.indices.clear();
        let request = ShapeRequest::new(path, transform, ShapeStyle::Fill(fill_rule))?;
        if let Some(hit) = self.geometry.find(&request) {
            self.bake(&hit.get().mesh, request.origin, color, opa);
            return Ok(());
        }
        self.fill_tess
            .fill_lyon(&commands_to_lyon(request.commands(), None), fill_rule)?;
        let mesh = self.fill_tess.take_mesh();
        self.bake(&mesh, request.origin, color, opa);
        self.geometry.retain(&request, &mesh);
        self.fill_tess.restore_mesh(mesh);
        Ok(())
    }

    pub fn stroke(
        &mut self,
        path: &[PathCmd],
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
        color: &Color,
        opa: u8,
    ) -> Result<(), RenderError> {
        self.verts.clear();
        self.indices.clear();
        let request = ShapeRequest::new(path, transform, ShapeStyle::Stroke(spec))?;
        if let Some(hit) = self.geometry.find(&request) {
            self.bake(&hit.get().mesh, request.origin, color, opa);
            return Ok(());
        }
        self.stroke_tess
            .stroke_lyon(&commands_to_lyon(request.commands(), None), spec)?;
        let mesh = self.stroke_tess.take_mesh();
        self.bake(&mesh, request.origin, color, opa);
        self.geometry.retain(&request, &mesh);
        self.stroke_tess.restore_mesh(mesh);
        Ok(())
    }

    pub fn dashed_stroke(
        &mut self,
        path: &[PathCmd],
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
        scratch: &mut StrokeScratch,
        color: &Color,
        opa: u8,
    ) -> Result<(), RenderError> {
        self.verts.clear();
        self.indices.clear();
        let request = ShapeRequest::new(path, transform, ShapeStyle::Stroke(spec))?;
        if let Some(hit) = self.geometry.find(&request) {
            self.bake(&hit.get().mesh, request.origin, color, opa);
            return Ok(());
        }
        let outline = scratch.outline(path, transform, spec);
        let normalized = ShapeRequest::at_origin(
            outline.commands(),
            None,
            ShapeStyle::Fill(FillRule::NonZero),
            request.origin,
        )?;
        self.fill_tess.fill_lyon(
            &commands_to_lyon(normalized.commands(), None),
            FillRule::NonZero,
        )?;
        let mesh = self.fill_tess.take_mesh();
        self.bake(&mesh, request.origin, color, opa);
        self.geometry.retain(&request, &mesh);
        self.fill_tess.restore_mesh(mesh);
        Ok(())
    }

    fn bake(
        &mut self,
        mesh: &VertexBuffers<LyonPoint, u32>,
        origin: Point,
        color: &Color,
        opa: u8,
    ) {
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
                position: SDL_FPoint {
                    x: p.x + origin.x.to_f32(),
                    y: p.y + origin.y.to_f32(),
                },
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

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/tessellation.rs"]
mod tests;
