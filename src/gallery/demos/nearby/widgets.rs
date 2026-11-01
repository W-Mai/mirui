use super::{
    model::{DisplayState, TextField},
    style::*,
};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign, TextOverflow, TextWrap};
use TextAlign::Center;
use TextField::Next;
use alloc::{borrow::ToOwned, string::String};

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

#[compose]
pub(super) fn caption<T, C>(
    content: T,
    color: C,
    size: u16,
    align: TextAlign,
    leading: i32,
) -> Entity
where
    T: Fn() -> String + 'static,
    C: Fn() -> ColorToken + 'static,
{
    ui! {
        text_block (content, color, size, align, leading, 1)
    }
}

#[compose]
pub(super) fn text_block<T, C>(
    content: T,
    color: C,
    size: u16,
    align: TextAlign,
    leading: i32,
    lines: u16,
) -> Entity
where
    T: Fn() -> String + 'static,
    C: Fn() -> ColorToken + 'static,
{
    ui! {
        Text (
            text: ${ content() },
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_width: 0,
            min_height: i32::from(size) + leading,
            font: font(size),
            font_size: size,
            text_color: ${ color() },
            paragraph: paragraph(align, lines),
            clip_children: true
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
pub(super) fn label(value: &str, size: u16, color: ColorToken, align: TextAlign) -> Entity {
    let value = value.to_owned();
    ui! {
        caption (move || value.clone(), move || color, size, align, LABEL_LEADING)
    }
}

#[compose]
pub(super) fn live_label(
    state: model!(DisplayState),
    field: TextField,
    size: u16,
    color: ColorToken,
    align: TextAlign,
) -> Entity {
    ui! {
        caption (move || state.text(field), move || color, size, align, LABEL_LEADING)
    }
}

#[compose]
pub(super) fn action_label(state: model!(DisplayState), size: u16) -> Entity {
    let content = state.clone();
    ui! {
        caption (
            move || content.text(TextField::HostButton),
            move || if state.action_enabled() { ON_ACCENT } else { FOREGROUND },
            size,
            Center,
            LABEL_LEADING
        )
    }
}

#[compose]
pub(super) fn guidance(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${
                if !state.next().is_empty() && state.next() != state.status() {
                    i32::from(metrics.small) + LABEL_LEADING
                } else {
                    0
                }
            },
            min_height: 0,
            shrink: 0.0
        ) {
            if ${ !state.next().is_empty() && state.next() != state.status() } {
                live_label (state, Next, metrics.small, SECONDARY, Center)
            }
        }
    }
}

#[compose]
pub(super) fn control_key(key: &str, metrics: Metrics, primary: bool) -> Entity {
    ui! {
        Text (
            key.to_owned(),
            width: Dimension::Content,
            min_width: metrics.keycap_size(),
            height: metrics.keycap_size(),
            shrink: 0.0,
            padding: Padding {
                left: metrics.control_gap().into(),
                right: metrics.control_gap().into(),
                ..Default::default()
            },
            bg_color: if primary { ACCENT } else { PANEL },
            font: font(metrics.small),
            font_size: metrics.small,
            text_color: if primary { ON_ACCENT } else { FOREGROUND },
            paragraph: paragraph(Center, 1)
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
pub(super) fn control_label<T>(content: T, metrics: Metrics) -> Entity
where
    T: Fn() -> String + 'static,
{
    ui! {
        Text (
            text: ${ content() },
            width: Dimension::Content,
            min_width: 0,
            padding: Padding {
                right: metrics.shortcut_gap().into(),
                ..Default::default()
            },
            max_width: if metrics.compact { 130 } else { 210 },
            height: Dimension::Content,
            font: font(metrics.small),
            font_size: metrics.small,
            text_color: FOREGROUND,
            paragraph: paragraph(TextAlign::Start, 1)
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
pub(super) fn shadow(metrics: Metrics) -> Entity {
    ui! {
        View (height: metrics.shadow_height(), shrink: 0.0, bg_color: DIVIDER)
    }
}
