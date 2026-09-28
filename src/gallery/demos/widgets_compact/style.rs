use crate::ui::Theme;
use crate::ui::widgets::{ParagraphStyle, TextAlign, TextVerticalAlign, TextWrap};

pub(super) struct CompactTheme(pub(super) Theme);

pub(super) fn bitmap_label(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}
