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
        width: u16,
        opa: Opa,
    },
    Arc {
        center: Point,
        radius: u16,
        start_angle: u16,
        end_angle: u16,
        color: Color,
        width: u16,
        opa: Opa,
    },
    Blit {
        pos: Point,
        data: &'a [u8], // RGBA
        width: u16,
        height: u16,
    },
}
