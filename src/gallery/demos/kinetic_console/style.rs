use crate::prelude::ColorToken;
use crate::ui::widgets::{ParagraphStyle, TextVerticalAlign, TextWrap};

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const CYAN: ColorToken = ColorToken::Primary;
pub(super) const BLUE: ColorToken = ColorToken::Secondary;
pub(super) const VIOLET: ColorToken = ColorToken::Tertiary;
pub(super) const AMBER: ColorToken = ColorToken::Success;
pub(super) const PINK: ColorToken = ColorToken::Error;

pub(super) fn header_label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}
