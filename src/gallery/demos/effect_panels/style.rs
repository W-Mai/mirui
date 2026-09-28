use crate::prelude::Color;

pub(super) fn tile_color(i: i32) -> Color {
    [
        Color::rgb(220, 60, 60),
        Color::rgb(220, 160, 40),
        Color::rgb(60, 200, 80),
        Color::rgb(40, 140, 220),
        Color::rgb(180, 80, 220),
        Color::rgb(40, 200, 200),
    ][(i % 6) as usize]
}
