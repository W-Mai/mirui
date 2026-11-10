use crate::prelude::ColorToken;
use crate::ui::widgets::{ParagraphStyle, TextVerticalAlign, TextWrap};

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
/// Selected segment of the mode switch.
pub(super) const RAISED: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
/// The single accent: the focused orbit, its trail and the intensity fill.
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
pub(super) const LIVE: ColorToken = ColorToken::Success;
pub(super) const HELD: ColorToken = ColorToken::Error;

pub(super) fn header_label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}
