use crate::prelude::*;

pub(super) const PAPER: Color = Color::rgb(240, 238, 230);
pub(super) const INK: Color = Color::rgb(36, 48, 47);
pub(super) const MUTED: Color = Color::rgb(111, 120, 113);
pub(super) const LINE: Color = Color::rgb(210, 214, 204);
pub(super) const CARD: Color = Color::rgb(253, 252, 246);
pub(super) const MINT: Color = Color::rgb(205, 231, 151);

pub(super) fn colors(index: u8) -> (Color, Color) {
    match index % 3 {
        0 => (Color::rgb(202, 221, 196), Color::rgb(87, 122, 87)),
        1 => (Color::rgb(195, 215, 236), Color::rgb(87, 125, 167)),
        _ => (Color::rgb(208, 196, 225), Color::rgb(119, 97, 151)),
    }
}
