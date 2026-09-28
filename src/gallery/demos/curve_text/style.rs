use crate::prelude::ColorToken;
use crate::render::font::{FontStack, FontToken};
use crate::ui::widgets::{ParagraphStyle, ShapingPolicy, TextDirection, TextOverflow, TextWrap};

pub(super) const UI: FontToken = FontToken::Custom("typography_ui");
const CJK: FontToken = FontToken::Custom("typography_cjk");
const ARABIC: FontToken = FontToken::Custom("typography_arabic");
const THAI: FontToken = FontToken::Custom("typography_thai");
const FALLBACKS: [FontToken; 3] = [CJK, ARABIC, THAI];

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const PANEL_ALT: ColorToken = ColorToken::Surface;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const CYAN: ColorToken = ColorToken::Primary;
pub(super) const VIOLET: ColorToken = ColorToken::Tertiary;
pub(super) const GOLD: ColorToken = ColorToken::Success;
pub(super) const LANE_COLORS: [ColorToken; 3] = [CYAN, VIOLET, GOLD];

pub(super) const ROUTE_LABEL: &str = "POSED GLYPHS / BOUNDED FALLBACK";

pub(super) fn mixed_stack() -> FontStack {
    FontStack::new(UI).with_fallbacks(&FALLBACKS)
}

pub(super) fn single_line(direction: TextDirection) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        overflow: TextOverflow::Ellipsis,
        direction,
        shaping: ShapingPolicy::Required,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

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
