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
}

impl TessellationCache {
    pub fn new() -> Self {
        Self {
            geometry: GeometryCache::new(1024 * 1024),
            fill_tess: FillTessellator::new(),
            stroke_tess: SharedStrokeTessellator::new(),
            verts: Vec::new(),
            indices: Vec::new(),
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
mod tests {
    use super::*;
    use crate::types::Fixed;

    #[test]
    fn primitive_and_dashed_cache_hits_skip_construction_workspaces() {
        use crate::render::path::{rounded_quad_commands, rounded_rect_commands};
        use crate::render::raster::{LineCap, LineJoin};
        let quad = [
            Point::new(0, 0),
            Point::new(80, 0),
            Point::new(80, 40),
            Point::new(0, 40),
        ];
        let mut cache = TessellationCache::new();
        let mut scratch = StrokeScratch::new();
        let spec = StrokeSpec {
            width: 2.into(),
            cap: LineCap::Round,
            join: LineJoin::Round,
            miter_limit: 4.into(),
            dash: &[5.into(), 3.into()],
            dash_scale: Fixed::ONE,
        };
        let color = Color::rgb(30, 80, 120);
        let mut draw = |scratch: &mut StrokeScratch| {
            let rect = rounded_rect_commands(0.into(), 0.into(), 80.into(), 40.into(), 6.into());
            cache
                .fill(rect.commands(), None, &color, 255, FillRule::NonZero)
                .unwrap();
            let rounded = rounded_quad_commands(&quad, 6.into());
            cache
                .fill(rounded.commands(), None, &color, 255, FillRule::NonZero)
                .unwrap();
            cache
                .dashed_stroke(rounded.commands(), None, spec, scratch, &color, 255)
                .unwrap();
        };
        draw(&mut scratch);
        // A fresh scratch has no flattened paths or stroke buffers to reuse.
        let mut untouched = StrokeScratch::new();
        assert_eq!(
            crate::test_support::tracked_allocations(|| draw(&mut untouched)),
            0
        );
        let mut uncached = TessellationCache::new();
        uncached.geometry = GeometryCache::new(0);
        let rounded = rounded_quad_commands(&quad, 6.into());
        uncached
            .dashed_stroke(rounded.commands(), None, spec, &mut scratch, &color, 255)
            .unwrap();
        assert_eq!(cache.indices, uncached.indices);
        assert_eq!(cache.verts.len(), uncached.verts.len());
        for (cached, direct) in cache.verts.iter().zip(&uncached.verts) {
            assert_eq!(
                (cached.position.x, cached.position.y),
                (direct.position.x, direct.position.y)
            );
        }
    }

    #[test]
    fn translated_recolored_mesh_hits_without_allocation_or_tessellation() {
        let path = Path::rounded_rect(0.into(), 0.into(), 40.into(), 20.into(), 3.into());
        let mut cache = TessellationCache::new();
        cache
            .fill(
                path.commands(),
                None,
                &Color::rgb(100, 20, 30),
                255,
                FillRule::NonZero,
            )
            .unwrap();
        let positions: Vec<_> = cache
            .verts
            .iter()
            .map(|v| (v.position.x, v.position.y))
            .collect();
        let translate = Transform {
            tx: Fixed::from_ratio(13, 2),
            ty: Fixed::from_int(-7),
            ..Transform::IDENTITY
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                cache
                    .fill(
                        path.commands(),
                        Some(&translate),
                        &Color::rgb(10, 80, 30),
                        128,
                        FillRule::NonZero,
                    )
                    .unwrap();
            }),
            0
        );
        assert_eq!(cache.geometry.inspect().cache_stats().hit_count, 1);
        for (vertex, original) in cache.verts.iter().zip(positions) {
            assert_eq!(vertex.position.x, original.0 + 6.5);
            assert_eq!(vertex.position.y, original.1 - 7.0);
            assert_eq!(
                (vertex.color.r, vertex.color.g, vertex.color.a),
                (10, 80, 128)
            );
        }
        let mut uncached = TessellationCache::new();
        uncached.geometry = GeometryCache::new(0);
        uncached
            .fill(
                path.commands(),
                Some(&translate),
                &Color::rgb(10, 80, 30),
                128,
                FillRule::NonZero,
            )
            .unwrap();
        assert_eq!(cache.indices, uncached.indices);
        for (a, b) in cache.verts.iter().zip(&uncached.verts) {
            assert_eq!((a.position.x, a.position.y), (b.position.x, b.position.y));
        }
        assert_eq!(uncached.geometry.inspect().cache_len(), 0);
    }
}
