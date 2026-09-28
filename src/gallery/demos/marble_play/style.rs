use crate::prelude::{Color, Dimension, Fixed, Padding};

pub(super) const BOARD_WIDTH: i32 = 480;
pub(super) const BOARD_HEIGHT: i32 = 199;
pub(super) const MUTED: Color = Color::rgb(137, 156, 123);
pub(super) const TEXT: Color = Color::rgb(225, 233, 214);

pub(super) fn color(value: u32) -> Color {
    Color::rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

pub(super) fn mix(a: u32, b: u32, amount: Fixed) -> Color {
    color(a).blend_with(color(b), amount)
}

pub(super) fn symmetric_padding(vertical: i32, horizontal: i32) -> Padding {
    Padding {
        top: Dimension::px(vertical),
        right: Dimension::px(horizontal),
        bottom: Dimension::px(vertical),
        left: Dimension::px(horizontal),
    }
}
