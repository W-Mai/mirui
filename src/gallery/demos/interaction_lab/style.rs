use crate::prelude::{ColorToken, Dimension, Fixed};
use crate::ui::widgets::{ParagraphStyle, TextOverflow, TextVerticalAlign, TextWrap};

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
pub(super) const ERROR: ColorToken = ColorToken::Error;

pub(super) fn bounded_text(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: if lines == 1 {
            TextWrap::NoWrap
        } else {
            TextWrap::Word
        },
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}

pub(super) fn ellipsis_label() -> ParagraphStyle {
    ParagraphStyle {
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(1),
        ..ParagraphStyle::label()
    }
}

pub(super) fn status_text() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        vertical_align: TextVerticalAlign::Center,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

pub(super) fn card_width(width: Fixed) -> Dimension {
    if width < Fixed::from_int(800) {
        Dimension::percent(100)
    } else {
        Dimension::percent(48)
    }
}

pub(super) fn gesture_cell_width(width: Fixed) -> Dimension {
    if width < Fixed::from_int(400) {
        Dimension::percent(48)
    } else {
        Dimension::percent(23)
    }
}
