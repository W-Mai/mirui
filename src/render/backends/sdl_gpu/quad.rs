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
use crate::render::path::Path;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Fixed, Point, Rect};

use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

impl SdlGpuRenderer<'_> {
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
        let path = Path::rounded_quad(&phys_q, phys_radius);
        self.tessellator.fill(&path, color, opa);
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
        let phys_width = (width * self.viewport.scale()).to_f32().max(1.0);
        let path = Path::rounded_quad(&phys_q, phys_radius);
        self.tessellator.stroke(&path, phys_width, color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    pub(super) fn blit_quad_inner(&mut self, src: &Texture, q: &[Point; 4], clip: &Rect, opa: u8) {
        if opa == 0 {
            return;
        }
        let sdl_fmt = match src.format {
            ColorFormat::RGBA8888 => sdl2::pixels::PixelFormatEnum::RGBA32,
            ColorFormat::BGRA8888 => sdl2::pixels::PixelFormatEnum::BGRA32,
            ColorFormat::RGB888 => sdl2::pixels::PixelFormatEnum::RGB24,
            ColorFormat::RGB565 => sdl2::pixels::PixelFormatEnum::RGB565,
            ColorFormat::RGB565Swapped => return,
        };
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

        let src_slice = src.buf.as_slice();
        let stride = src.stride;
        let src_width = src.width as u32;
        let src_height = src.height as u32;

        // SDL_RenderGeometry modulates the texture sample by per-vertex
        // color, so packing opa into vertex.a applies group opacity over
        // the full warp without a separate set_alpha_mod call.
        let tint = SDL_Color {
            r: 255,
            g: 255,
            b: 255,
            a: opa,
        };
        let verts: [SDL_Vertex; 4] = [
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

        let canvas = &mut *self.canvas;
        self.label_cache.with_creator(|creator| {
            let mut tex = match creator.create_texture_streaming(sdl_fmt, src_width, src_height) {
                Ok(t) => t,
                Err(_) => return,
            };
            if tex.update(None, src_slice, stride).is_err() {
                return;
            }
            tex.set_blend_mode(sdl2::render::BlendMode::Blend);
            canvas.set_clip_rect(sdl_clip);
            unsafe {
                sdl2_sys::SDL_RenderGeometry(
                    canvas.raw(),
                    tex.raw(),
                    verts.as_ptr(),
                    verts.len() as _,
                    indices.as_ptr(),
                    indices.len() as _,
                );
            }
            canvas.set_clip_rect(None);
        });
    }
}
