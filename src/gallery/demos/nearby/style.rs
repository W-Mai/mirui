use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, TextAlign, TextOverflow, TextWrap};

pub(super) const FONT: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("nearby_menu");
pub(super) const TITLE_FONT: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("nearby_title");

pub(super) fn font(size: u16) -> crate::render::font::FontToken {
    if size == 24 { TITLE_FONT } else { FONT }
}
pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
pub(super) const ON_ACCENT: ColorToken = ColorToken::OnPrimary;
pub(super) const ACCENT_DIM: ColorToken = ColorToken::Custom("sky");
pub(super) const FOREGROUND: ColorToken = ColorToken::OnSurface;
pub(super) const SECONDARY: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const DISABLED: ColorToken = SECONDARY;
pub(super) const DIVIDER: ColorToken = ColorToken::Outline;
pub(super) const WARNING: ColorToken = ColorToken::Custom("gold");
pub(super) const WARNING_PANEL: ColorToken = PANEL;
pub(super) const SKY: ColorToken = ColorToken::Custom("sky");
pub(super) const HILL: ColorToken = ColorToken::Custom("hill");
pub(super) const DEEP: ColorToken = ColorToken::Custom("deep");
pub(super) const CLAY: ColorToken = ColorToken::Custom("clay");
pub(super) const HEADER_TEXT: ColorToken = ColorToken::Custom("header-text");
pub(super) const HEADER_DETAIL: ColorToken = ColorToken::Custom("header-detail");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Metrics {
    pub(super) compact: bool,
    pub(super) tiny: bool,
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
        Self::for_height(480)
    }
}

impl Metrics {
    fn sized(self, tiny: i32, compact: i32, regular: i32) -> i32 {
        if self.tiny {
            tiny
        } else if self.compact {
            compact
        } else {
            regular
        }
    }
    pub(super) fn header_height(self) -> i32 {
        self.sized(44, 52, 72)
    }
    pub(super) fn logo_size(self) -> i32 {
        self.sized(30, 30, 48)
    }
    pub(super) fn header_padding(self) -> Padding {
        Padding {
            top: 4.into(),
            bottom: 4.into(),
            left: self.sized(16, 16, 24).into(),
            right: self.sized(16, 16, 24).into(),
        }
    }
    pub(super) fn modal_padding(self) -> Padding {
        let horizontal = Dimension::Percent(if self.compact {
            Fixed::from_ratio(5, 2)
        } else {
            Fixed::from_ratio(15, 4)
        });
        let vertical = Dimension::Percent(if self.compact {
            Fixed::from_ratio(5, 2)
        } else {
            Fixed::from_ratio(20, 3)
        });
        Padding {
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }
    pub(super) fn for_height(height: u16) -> Self {
        let compact = height < 400;
        Self {
            compact,
            tiny: height < 300,
            title: if compact { 12 } else { 24 },
            body: 12,
            small: 12,
            row_title: if compact { 12 } else { 24 },
            row_detail: 12,
            gap: if compact { 3 } else { 8 },
            inset: if compact { 8 } else { 18 },
        }
    }
    pub(super) fn walker_height(self) -> i32 {
        if self.tiny {
            22
        } else if self.compact {
            38
        } else {
            46
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
