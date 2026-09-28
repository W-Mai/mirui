use crate::prelude::Color;
use crate::ui::widgets::{ParagraphStyle, TextAlign};

pub(super) const INK: Color = Color::rgb(24, 34, 45);
pub(super) const BACKGROUND: Color = Color::rgb(239, 239, 233);
pub(super) const PANEL: Color = Color::rgb(250, 250, 244);
pub(super) const SPACE: Color = Color::rgb(17, 28, 41);
pub(super) const LINE: Color = Color::rgb(188, 199, 201);
pub(super) const MUTED: Color = Color::rgb(110, 126, 139);
pub(super) const ORANGE: Color = Color::rgb(244, 139, 54);
pub(super) const CYAN: Color = Color::rgb(65, 210, 204);
pub(super) const VIOLET: Color = Color::rgb(147, 112, 222);

pub(super) fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}
