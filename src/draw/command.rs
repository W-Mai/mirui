use crate::types::{Color, Opa, Point, Rect};

pub enum DrawCommand {
    Fill {
        area: Rect,
        color: Color,
        radius: u16,
        opa: Opa,
    },
    Border {
        area: Rect,
        color: Color,
        width: u16,
        radius: u16,
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
}
