use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, TextAlign, TextOverflow, TextWrap};

pub(super) const FONT: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("nearby_menu");
pub(super) const TITLE_FONT: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("nearby_title");

pub(super) fn font(size: u16) -> crate::render::font::FontToken {
    if size == 24 { TITLE_FONT } else { FONT }
}
pub(super) const BACKGROUND: Color = Color::rgb(242, 235, 221);
pub(super) const PANEL: Color = Color::rgb(231, 221, 200);
pub(super) const ACCENT: Color = Color::rgb(166, 61, 50);
pub(super) const ACCENT_DIM: Color = Color::rgb(234, 207, 192);
pub(super) const FOREGROUND: Color = Color::rgb(41, 37, 32);
pub(super) const SECONDARY: Color = Color::rgb(112, 98, 83);
pub(super) const DISABLED: Color = Color::rgb(131, 117, 99);
pub(super) const DIVIDER: Color = Color::rgb(204, 192, 169);
pub(super) const WARNING: Color = Color::rgb(152, 106, 32);
pub(super) const WARNING_PANEL: Color = Color::rgb(233, 218, 186);

#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub(super) title: u16,
    pub(super) body: u16,
    pub(super) small: u16,
    pub(super) row_title: u16,
    pub(super) row_detail: u16,
    pub(super) gap: i32,
    pub(super) inset: i32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            title: 24,
            body: 12,
            small: 12,
            row_title: 24,
            row_detail: 12,
            gap: 6,
            inset: 12,
        }
    }
}

#[component]
pub(super) struct StaticCache;

pub(super) fn paragraph(align: TextAlign, lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        align,
        vertical_align: crate::ui::widgets::text::TextVerticalAlign::Center,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..Default::default()
    }
}
