use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Opa, Point, Rect, Transform};

/// Draw operation produced by `render_system` and consumed by `Renderer::draw`.
///
/// All coordinate fields (`area`, `pos`, path points, `radius`, `width`) are
/// in **logical pixels**. Each variant carries a [`Transform`] (2D widget
/// affine). Variants that can participate in a 3D warp also carry an
/// optional pre-projected `quad: Option<[Point; 4]>`; when present, the
/// renderer uses the quad directly and ignores `area + transform` for
/// geometry. Renderers may `unimplemented!()` on transform classes they
/// don't handle (see `SwRenderer::draw_transformed` for what the software
/// backend covers today).
pub enum DrawCommand<'a> {
    Fill {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        radius: Fixed,
        opa: Opa,
    },
    Border {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        width: Fixed,
        radius: Fixed,
        opa: Opa,
    },
    Label {
        pos: Point,
        transform: Transform,
        text: &'a [u8],
        font: &'a Font,
        color: Color,
        opa: Opa,
    },
    Line {
        p1: Point,
        p2: Point,
        transform: Transform,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    /// Stroked arc on a circle (center, radius). Angles in degrees, CCW.
    Arc {
        center: Point,
        transform: Transform,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    /// Blit `texture` at `pos`, scaling (nearest) to `size` logical pixels.
    Blit {
        pos: Point,
        size: Point,
        transform: Transform,
        quad: Option<[Point; 4]>,
        texture: &'a Texture<'a>,
        opa: Opa,
    },
    /// Fill the closed region described by `path`. Path vertices are in
    /// logical pixels; under non-translate transforms the backend may
    /// fall back to `unimplemented!` (same policy as `Arc`).
    FillPath {
        path: &'a Path,
        transform: Transform,
        color: Color,
        opa: Opa,
    },
}

impl DrawCommand<'_> {
    #[inline]
    pub fn transform(&self) -> Transform {
        match *self {
            Self::Fill { transform, .. }
            | Self::Border { transform, .. }
            | Self::Label { transform, .. }
            | Self::Line { transform, .. }
            | Self::Arc { transform, .. }
            | Self::Blit { transform, .. }
            | Self::FillPath { transform, .. } => transform,
        }
    }
}
