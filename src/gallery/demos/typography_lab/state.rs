use crate::prelude::{Entity, Fixed, Signal};
use crate::ui::widgets::{ParagraphStyle, ShapingPolicy, TextAlign, TextOverflow, TextWrap};

#[derive(Clone, Copy)]
pub(super) struct TypographyNodes {
    pub(super) path_overlay: Entity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TypographyState {
    pub(super) ppem: u16,
    pub(super) width: u16,
    pub(super) wrap: TextWrap,
    pub(super) align: TextAlign,
    pub(super) overflow: TextOverflow,
}

impl Default for TypographyState {
    fn default() -> Self {
        Self {
            ppem: 24,
            width: 480,
            wrap: TextWrap::Word,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
        }
    }
}

impl TypographyState {
    pub(super) fn paragraph(self) -> ParagraphStyle {
        ParagraphStyle {
            wrap: self.wrap,
            align: self.align,
            overflow: self.overflow,
            max_lines: Some(2),
            shaping: ShapingPolicy::Required,
            ..ParagraphStyle::default()
        }
    }

    pub(super) fn wrap_label(self) -> &'static str {
        match self.wrap {
            TextWrap::NoWrap => "NO WRAP",
            TextWrap::Word => "WORD",
            TextWrap::Grapheme => "GRAPHEME",
        }
    }

    pub(super) fn align_label(self) -> &'static str {
        match self.align {
            TextAlign::Start => "START",
            TextAlign::Center => "CENTER",
            TextAlign::End => "END",
            TextAlign::Justify => "JUSTIFY",
        }
    }

    pub(super) fn overflow_label(self) -> &'static str {
        match self.overflow {
            TextOverflow::Clip => "CLIP",
            TextOverflow::Ellipsis => "ELLIPSIS",
        }
    }
}

pub(super) enum TypographyAction {
    SetPpem(Fixed),
    SetWidth(Fixed),
    CycleWrap,
    CycleAlign,
    ToggleOverflow,
}

impl TypographyAction {
    pub(super) fn publish(self, state: &Signal<TypographyState>) {
        let mut next = state.get_untracked();
        match self {
            Self::SetPpem(value) => {
                next.ppem = value.round().to_int().clamp(10, 64) as u16;
            }
            Self::SetWidth(value) => {
                next.width = value.round().to_int().clamp(220, 560) as u16;
            }
            Self::CycleWrap => {
                next.wrap = match next.wrap {
                    TextWrap::NoWrap => TextWrap::Word,
                    TextWrap::Word => TextWrap::Grapheme,
                    TextWrap::Grapheme => TextWrap::NoWrap,
                };
            }
            Self::CycleAlign => {
                next.align = match next.align {
                    TextAlign::Start => TextAlign::Center,
                    TextAlign::Center => TextAlign::End,
                    TextAlign::End => TextAlign::Justify,
                    TextAlign::Justify => TextAlign::Start,
                };
            }
            Self::ToggleOverflow => {
                next.overflow = match next.overflow {
                    TextOverflow::Clip => TextOverflow::Ellipsis,
                    TextOverflow::Ellipsis => TextOverflow::Clip,
                };
            }
        }
        if next != state.get_untracked() {
            state.set(next);
        }
    }
}
