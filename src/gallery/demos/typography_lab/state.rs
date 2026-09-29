use crate::prelude::{Fixed, Point};
use crate::ui::widgets::{ParagraphStyle, ShapingPolicy, TextAlign, TextOverflow, TextWrap};

#[crate::model]
#[derive(Debug, Eq, PartialEq)]
pub(super) struct TypographyModel {
    #[observe]
    pub(super) ppem: u16,
    #[observe]
    pub(super) width: u16,
    #[observe]
    pub(super) wrap: TextWrap,
    #[observe]
    pub(super) align: TextAlign,
    #[observe]
    pub(super) overflow: TextOverflow,
    #[observe]
    pub(super) path_probe: Option<Point>,
}

impl Default for TypographyModel {
    fn default() -> Self {
        Self {
            ppem: 24,
            width: 480,
            wrap: TextWrap::Word,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
            path_probe: None,
        }
    }
}

#[crate::model]
impl TypographyModel {
    pub(super) fn set_ppem(&mut self, value: Fixed) {
        self.ppem = value.round().to_int().clamp(10, 64) as u16;
    }

    pub(super) fn set_width(&mut self, value: Fixed) {
        self.width = value.round().to_int().clamp(220, 560) as u16;
    }

    pub(super) fn cycle_wrap(&mut self) {
        self.wrap = match self.wrap {
            TextWrap::NoWrap => TextWrap::Word,
            TextWrap::Word => TextWrap::Grapheme,
            TextWrap::Grapheme => TextWrap::NoWrap,
        };
    }

    pub(super) fn cycle_align(&mut self) {
        self.align = match self.align {
            TextAlign::Start => TextAlign::Center,
            TextAlign::Center => TextAlign::End,
            TextAlign::End => TextAlign::Justify,
            TextAlign::Justify => TextAlign::Start,
        };
    }

    pub(super) fn toggle_overflow(&mut self) {
        self.overflow = match self.overflow {
            TextOverflow::Clip => TextOverflow::Ellipsis,
            TextOverflow::Ellipsis => TextOverflow::Clip,
        };
    }

    pub(super) fn set_path_probe(&mut self, point: Point) {
        self.path_probe = Some(point);
    }
}

pub(super) fn live_paragraph(
    wrap: TextWrap,
    align: TextAlign,
    overflow: TextOverflow,
) -> ParagraphStyle {
    ParagraphStyle {
        wrap,
        align,
        overflow,
        max_lines: Some(2),
        shaping: ShapingPolicy::Required,
        ..ParagraphStyle::default()
    }
}

pub(super) const fn wrap_label(wrap: TextWrap) -> &'static str {
    match wrap {
        TextWrap::NoWrap => "NO WRAP",
        TextWrap::Word => "WORD",
        TextWrap::Grapheme => "GRAPHEME",
    }
}

pub(super) const fn align_label(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Start => "START",
        TextAlign::Center => "CENTER",
        TextAlign::End => "END",
        TextAlign::Justify => "JUSTIFY",
    }
}

pub(super) const fn overflow_label(overflow: TextOverflow) -> &'static str {
    match overflow {
        TextOverflow::Clip => "CLIP",
        TextOverflow::Ellipsis => "ELLIPSIS",
    }
}
