//! Quad-path rendering for the SDL GPU backend.
//!
//! The software backend rasterises `DrawCommand.{Fill,Border,Blit}.quad`
//! directly; here we lean on SDL_RenderGeometry: fills/strokes go through
//! the existing lyon tessellator, blits feed SDL a 2-triangle mesh with
//! texture UVs so the GPU handles the warp natively.
//!
//! Blit warp is affine (linear UV interpolation across each triangle),
//! which is visibly wrong under hard perspective tilt — good enough for
//! cover-flow-style tilts; tessellation subdivision or a perspective-
//! correct shader path can come later.

use super::{SdlGpuRenderer, sdl_pixel_rect};
use crate::render::path::rounded_quad_commands;
use crate::render::renderer::RenderError;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Point, Rect};

use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn fill_quad_inner(
        &mut self,
        q: &[Point; 4],
        radius: Fixed,
        color: &Color,
        opa: u8,
        clip: &Rect,
    ) {
        // DrawCommand carries quads in logical pixels — the viewport
        // still owns the scale → physical conversion.
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let phys_radius = radius * self.viewport.scale();
        let path = rounded_quad_commands(&phys_q, phys_radius);
        if let Err(error) = self.tessellator.fill(
            path.commands(),
            None,
            color,
            opa,
            crate::render::raster::FillRule::NonZero,
        ) {
            self.failure.get_or_insert(error);
            return;
        }
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn stroke_quad_inner(
        &mut self,
        q: &[Point; 4],
        width: Fixed,
        radius: Fixed,
        color: &Color,
        opa: u8,
        clip: &Rect,
    ) {
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let phys_radius = radius * self.viewport.scale();
        let phys_width = (width * self.viewport.scale()).max(Fixed::ONE);
        let path = rounded_quad_commands(&phys_q, phys_radius);
        if let Err(error) = self.tessellator.stroke(
            path.commands(),
            None,
            crate::render::raster::StrokeSpec {
                width: phys_width,
                cap: crate::render::raster::LineCap::Butt,
                join: crate::render::raster::LineJoin::Miter,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
            color,
            opa,
        ) {
            self.failure.get_or_insert(error);
            return;
        }
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn blit_quad_inner(&mut self, src: &Texture, q: &[Point; 4], clip: &Rect, opa: u8) {
        if opa == 0 {
            return;
        }
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let Some(sdl_clip) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
            return;
        };

        // SDL_RenderGeometry modulates the texture sample by per-vertex
        // color, so packing opa into vertex.a applies group opacity over
        // the full warp without a separate set_alpha_mod call.
        let tint = SDL_Color {
            r: 255,
            g: 255,
            b: 255,
            a: opa,
        };
        let mut verts: [SDL_Vertex; 4] = [
            SDL_Vertex {
                position: SDL_FPoint {
                    x: phys_q[0].x.to_f32(),
                    y: phys_q[0].y.to_f32(),
                },
                color: tint,
                tex_coord: SDL_FPoint { x: 0.0, y: 0.0 },
            },
            SDL_Vertex {
                position: SDL_FPoint {
                    x: phys_q[1].x.to_f32(),
                    y: phys_q[1].y.to_f32(),
                },
                color: tint,
                tex_coord: SDL_FPoint { x: 1.0, y: 0.0 },
            },
            SDL_Vertex {
                position: SDL_FPoint {
                    x: phys_q[2].x.to_f32(),
                    y: phys_q[2].y.to_f32(),
                },
                color: tint,
                tex_coord: SDL_FPoint { x: 1.0, y: 1.0 },
            },
            SDL_Vertex {
                position: SDL_FPoint {
                    x: phys_q[3].x.to_f32(),
                    y: phys_q[3].y.to_f32(),
                },
                color: tint,
                tex_coord: SDL_FPoint { x: 0.0, y: 1.0 },
            },
        ];
        let indices: [i32; 6] = [0, 1, 2, 0, 2, 3];

        let uploaded = self.texture_cache.with_texture(
            src,
            crate::render::command::CompositeMode::SourceOver,
            |tex, alpha| {
                let mut canvas = self.device.canvas()?;
                if alpha == crate::render::TargetAlpha::Straight {
                    tex.set_blend_mode(sdl2::render::BlendMode::Blend);
                } else {
                    for vertex in &mut verts {
                        vertex.color.r = opa;
                        vertex.color.g = opa;
                        vertex.color.b = opa;
                    }
                }
                tex.set_alpha_mod(255);
                tex.set_color_mod(255, 255, 255);
                canvas.set_clip_rect(sdl_clip);
                let rendered = unsafe {
                    sdl2_sys::SDL_RenderGeometry(
                        canvas.raw(),
                        tex.raw(),
                        verts.as_ptr(),
                        verts.len() as _,
                        indices.as_ptr(),
                        indices.len() as _,
                    ) == 0
                };
                canvas.set_clip_rect(None);
                rendered.then_some(()).ok_or(RenderError::BackendFailure)
            },
        );
        if let Err(error) = uploaded.and_then(core::convert::identity) {
            self.failure.get_or_insert(error);
        }
    }
}
