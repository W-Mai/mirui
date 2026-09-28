use crate::prelude::*;
use crate::ui::widgets::{
    ParagraphStyle, ShapingPolicy, TextAlign, TextDirection, TextVerticalAlign, TextWrap,
};

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const CYAN: ColorToken = ColorToken::Primary;

pub(super) fn bitmap_line(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        direction: TextDirection::LeftToRight,
        shaping: ShapingPolicy::Simple,
        ..ParagraphStyle::default()
    }
}
