//! Bridge mirui's fixed-point [`Path`](crate::render::path::Path) to lyon
//! tessellation output that can be fed to SDL_RenderGeometry. All the
//! lyon-specific machinery lives here so the rest of the GPU backend
//! only depends on raw `SDL_Vertex` / index pairs.
//!
//! Every call reuses the FillTessellator / StrokeTessellator instances
//! and the vertex / index buffers: tessellating a complex path allocates
//! once on first use and then amortises to zero.

use alloc::vec::Vec;

use lyon::math::{Point as LyonPoint, point as lyon_point};
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::render::path::{Path, PathCmd};
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

    pub fn fill(&mut self, path: &Path, transform: Option<&Transform>, color: &Color, opa: u8) {
        self.lyon_verts.vertices.clear();
        self.lyon_verts.indices.clear();
        let lyon_path = to_lyon_path(path, transform);
        let _ = self.fill_tess.tessellate_path(
            &lyon_path,
            &FillOptions::tolerance(1.0),
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

fn to_lyon_path(path: &Path, transform: Option<&Transform>) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut subpath_open = false;

    let p = |pt: crate::types::Point| -> LyonPoint {
        let pt = match transform {
            Some(tf) => tf.apply_point(pt),
            None => pt,
        };
        lyon_point(pt.x.to_f32(), pt.y.to_f32())
    };

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
