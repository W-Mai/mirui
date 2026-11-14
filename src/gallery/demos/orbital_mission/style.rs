use crate::prelude::Color;
use crate::ui::widgets::{ParagraphStyle, TextAlign};

pub(super) const INK: Color = Color::rgb(24, 34, 45);
pub(super) const BACKGROUND: Color = Color::rgb(239, 239, 233);
pub(super) const PANEL: Color = Color::rgb(250, 250, 244);
pub(super) const SPACE: Color = Color::rgb(17, 28, 41);
/// Buttons on the dark plan panel.
pub(super) const DEEP: Color = Color::rgb(31, 50, 67);
pub(super) const LINE: Color = Color::rgb(188, 199, 201);
pub(super) const MUTED: Color = Color::rgb(110, 126, 139);
pub(super) const ORANGE: Color = Color::rgb(244, 139, 54);
pub(super) const CYAN: Color = Color::rgb(65, 210, 204);
pub(super) const VIOLET: Color = Color::rgb(147, 112, 222);
pub(super) const PLANET: Color = Color::rgb(255, 221, 128);
/// The planned path ahead of the craft.
pub(super) const COAST: Color = Color::rgba(188, 199, 201, 150);
pub(super) const CLEAR: Color = Color::rgba(0, 0, 0, 0);

pub(super) fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

pub(super) fn end_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::End)
}

pub(super) fn center_style() -> ParagraphStyle {
    ParagraphStyle::label()
}

/// Wrapping text, such as a mission description.
pub(super) fn body_style() -> ParagraphStyle {
    ParagraphStyle::default().with_line_height(15)
}
