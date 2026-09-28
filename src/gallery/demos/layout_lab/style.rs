use crate::prelude::ColorToken;
use crate::ui::widgets::{ParagraphStyle, TextOverflow, TextWrap};

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const PANEL_ALT: ColorToken = ColorToken::Surface;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const CYAN: ColorToken = ColorToken::Primary;
pub(super) const BLUE: ColorToken = ColorToken::Secondary;
pub(super) const VIOLET: ColorToken = ColorToken::Tertiary;
pub(super) const GOLD: ColorToken = ColorToken::Success;

pub(super) fn bounded_body(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}
