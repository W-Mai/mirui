use crate::prelude::*;

pub(super) const BACKGROUND: Color = Color::rgb(25, 33, 34);
pub(super) const HEADER: Color = Color::rgb(29, 39, 39);
pub(super) const BOARD: Color = Color::rgb(21, 29, 32);
pub(super) const TILE: Color = Color::rgb(32, 44, 46);
pub(super) const PANEL: Color = Color::rgb(38, 49, 54);
pub(super) const PANEL_BORDER: Color = Color::rgb(58, 71, 74);
pub(super) const CONTROL: Color = Color::rgb(42, 57, 51);
pub(super) const CONTROL_PRESSED: Color = Color::rgb(246, 214, 135);
pub(super) const ACCENT: Color = Color::rgb(246, 214, 135);
pub(super) const LIGHT: Color = Color::rgb(247, 239, 196);
pub(super) const SUCCESS: Color = Color::rgb(189, 225, 154);
pub(super) const TEXT: Color = Color::rgb(225, 234, 225);
pub(super) const MUTED: Color = Color::rgb(148, 169, 159);

pub(super) fn padding(vertical: i32, horizontal: i32) -> Padding {
    Padding {
        top: Dimension::px(vertical),
        right: Dimension::px(horizontal),
        bottom: Dimension::px(vertical),
        left: Dimension::px(horizontal),
    }
}
