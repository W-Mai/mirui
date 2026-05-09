use crate::draw::texture::Texture;
use crate::types::{Color, Fixed, Opa, Point, Rect};

pub enum DrawCommand<'a> {
    Fill {
        area: Rect,
        color: Color,
        radius: Fixed,
        opa: Opa,
    },
    Border {
        area: Rect,
        color: Color,
        width: Fixed,
        radius: Fixed,
        opa: Opa,
    },
    Label {
        pos: Point,
        text: &'a [u8],
        color: Color,
        opa: Opa,
    },
    Line {
        p1: Point,
        p2: Point,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    /// Stroked arc on a circle (center, radius). Angles in degrees, CCW.
    Arc {
        center: Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    Blit {
        pos: Point,
        texture: &'a Texture<'a>,
    },
}
