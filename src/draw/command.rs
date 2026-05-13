use crate::draw::texture::Texture;
use crate::types::{Color, Fixed, Opa, Point, Rect, Transform};

/// Draw operation produced by `render_system` and consumed by `Renderer::draw`.
///
/// All coordinate fields (`area`, `pos`, path points, `radius`, `width`) are
/// in **logical pixels**. Each variant carries a [`Transform`] (Layer 2
/// widget-affine) that for the v0.7 pipeline is always
/// [`Transform::IDENTITY`]; backends must handle identity and may
/// `unimplemented!()` on non-identity until the widget-transform spec lands.
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
            | Self::Blit { transform, .. } => transform,
        }
    }
}
