use crate::prelude::Color;

pub(super) fn tile_color(i: i32) -> Color {
    [
        Color::rgb(238, 122, 112),
        Color::rgb(242, 184, 64),
        Color::rgb(146, 204, 110),
        Color::rgb(90, 140, 220),
        Color::rgb(112, 130, 158),
        Color::rgb(68, 204, 188),
    ][(i % 6) as usize]
}
