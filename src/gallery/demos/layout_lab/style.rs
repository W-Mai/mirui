use crate::prelude::ColorToken;
use crate::ui::widgets::{ParagraphStyle, TextOverflow, TextWrap};

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const PANEL_ALT: ColorToken = ColorToken::Surface;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
/// Neutral fill for secondary blocks and the selected state.
pub(super) const RAISED: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
/// The one accent, kept for the block each card is about.
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
/// A mid-grey block that reads apart from both the accent and the panel.
pub(super) const SLATE: ColorToken = ColorToken::Secondary;

pub(super) fn bounded_body(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}
