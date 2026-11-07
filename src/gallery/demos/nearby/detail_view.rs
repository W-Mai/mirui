use super::actions::{ActionAnchor, ActionLayout, action_icon};
use super::launcher::Launcher;
use super::style::*;
use super::widgets::shadow;
use crate::prelude::*;
use crate::ui::widgets::{Text, TextAlign::Start};
use alloc::string::String;

#[compose]
pub(super) fn details_panel(state: model!(Launcher), metrics: Metrics) -> Entity {
    use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            Row (
                height: 12 + LABEL_LEADING,
                shrink: 0.0,
                min_width: 0,
                column_gap: metrics.gap
            ) {
                Text (
                    text: ${ state.detail_heading() },
                    grow: 1.0,
                    min_width: 0,
                    font: font(12),
                    font_size: 12,
                    text_color: SECONDARY,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                Text (
                    text: ${ state.detail_platform() },
                    width: Dimension::Content,
                    shrink: 0.0,
                    font: font(12),
                    font_size: 12,
                    text_color: SECONDARY
                ) [
                    FONT_PAINT,
                ]
            }
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                clip_children: true,
                scroll_offset: ${
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: state.detail_offset(),
                    }
                }
            ) [
                ScrollOffset::default(),
                ScrollConfig {
                    elastic: false,
                    ..Default::default()
                },
                ActionAnchor::Reading,
            ] {
                walk ${ 0..state.detail_count() } with index by index {
                    detail_row (state, index, metrics)
                }
            } on SizeChanged { state.detail_viewport(event.new.height); }
        }
    }
}

#[compose]
fn detail_row(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${
                state.detail_size(index, metrics.gap)
                    + Fixed::from_int(metrics.shadow_height() + metrics.gap)
            },
            shrink: 0.0,
            min_width: 0,
            padding: Padding {
                bottom: Dimension::px(metrics.gap),
                ..Default::default()
            },
            transform: ${
                crate::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.menu_selection() == index {
                            super::motion::hop(state.selection_age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            }
        ) {
            Row (
                height: ${ state.detail_size(index, metrics.gap) },
                shrink: 0.0,
                min_width: 0,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                bg_color: ${ if state.menu_selection() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == index { ACCENT } else { DIVIDER } }
            ) [
                ActionAnchor::Item {
                    index,
                    layout: ActionLayout::Details,
                },
            ] {
                View (width: 3, height: 40, shrink: 0.0, bg_color: ACCENT)
                Column (grow: 1.0, min_width: 0, height: Dimension::percent(100), row_gap: 4) {
                    detail_text (state, index, 0, 24)
                    detail_text (state, index, 1, 12)
                    if ${ state.menu_selection() == index && state.detail_expanded() } {
                        View (height: 1, bg_color: DIVIDER)
                        Column (
                            height: ${ state.detail_facts_size(index, metrics.gap) },
                            shrink: 0.0,
                            min_width: 0,
                            row_gap: metrics.gap
                        ) {
                            walk ${ 0..state.detail_fact_count(index) } with field by field {
                                detail_fact (state, index, field, metrics)
                            }
                        }
                    }
                }
                Column (width: 24, height: 24, shrink: 0.0) {
                    action_icon (state, index)
                }
            } on Tap {
                state.select_action(index);
                state.confirm_detail();
            }
            shadow (metrics)
        } on SizeChanged { state.detail_measure(index, event.new.height); }
    }
}

#[compose]
fn detail_text(state: model!(Launcher), index: usize, part: usize, size: u16) -> Entity {
    ui! {
        Text (
            text: ${
                match part {
                    0 => state.item(index).map(|item| item.name.into()).unwrap_or_default(),
                    1 => state.item(index).map(|item| item.value).unwrap_or_default(),
                    _ => String::new(),
                }
            },
            height: Dimension::Content,
            width: Dimension::percent(100),
            min_height: i32::from(size) + ROW_LEADING,
            min_width: 0,
            shrink: 0.0,
            font: font(size),
            font_size: size,
            text_color: if part == 1 { SECONDARY } else { FOREGROUND },
            paragraph: crate::ui::widgets::ParagraphStyle {
                wrap: crate::ui::widgets::TextWrap::Grapheme,
                max_lines: None,
                ..Default::default()
            }
        ) [
            FONT_PAINT,
        ] on SizeChanged { state.detail_text_size(index, part, event.new.height); }
    }
}

#[compose]
fn detail_fact(state: model!(Launcher), index: usize, field: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${ state.detail_fact_height(index, field) + Fixed::ONE },
            shrink: 0.0,
            min_width: 0
        ) {
            Row (
                height: ${ state.detail_fact_height(index, field) },
                shrink: 0.0,
                min_width: 0,
                column_gap: metrics.gap
            ) {
                Column (width: Dimension::percent(20), min_width: 56, shrink: 0.0) {
                    detail_fact_text (state, index, field, false)
                }
                Column (grow: 1.0, min_width: 0) {
                    detail_fact_text (state, index, field, true)
                }
            }
            View (height: 1, shrink: 0.0, bg_color: DIVIDER)
        }
    }
}

#[compose]
fn detail_fact_text(state: model!(Launcher), index: usize, field: usize, value: bool) -> Entity {
    ui! {
        Text (
            text: ${
                {
                    let fact = state.detail_fact(index, field);
                    if value { fact.value } else { fact.label }
                }
            },
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_height: 12 + ROW_LEADING,
            min_width: 0,
            shrink: 0.0,
            font: font(12),
            font_size: 12,
            text_color: if value { FOREGROUND } else { SECONDARY },
            paragraph: crate::ui::widgets::ParagraphStyle {
                wrap: crate::ui::widgets::TextWrap::Grapheme,
                max_lines: None,
                ..Default::default()
            }
        ) [
            FONT_PAINT,
        ] on SizeChanged { state.detail_fact_measure(index, field, value, event.new.height); }
    }
}
