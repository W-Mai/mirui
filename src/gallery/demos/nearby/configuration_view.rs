use super::configuration::Editor;
use super::configuration::schema::Section;
use super::{
    actions::{ActionAnchor, ActionLayout},
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{Image, Text, TextAlign::Start};
use alloc::borrow::ToOwned;

const TITLE_SIZE: u16 = 24;
const BODY_SIZE: u16 = 12;

#[component]
pub(super) struct ConfigScroll {
    state: model!(Editor),
}
#[system(order=Animation)]
fn scroll_config(nodes: query!(Entity, ConfigScroll, ScrollOffset), commands: commands!()) {
    for (entity, node, offset) in nodes {
        let y = node.state.offset();
        if node.state.active() && offset.y != y {
            commands.set::<crate::ui::property::prop::ScrollOffset>(
                entity,
                ScrollOffset { x: Fixed::ZERO, y },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_scroll_repaints_without_geometry_demand_and_ignores_inactive_readers() {
        for active in [true, false] {
            super::super::tests::assert_scroll_projection(config_scroll::view(), active, |app| {
                let mut state = Editor::default();
                if active {
                    state.sync(Some(&super::super::configuration::fixture()));
                }
                state.offset = Fixed::from_int(16);
                ConfigScroll {
                    state: app.add_model(state).finish(),
                }
            });
        }
    }
}
#[view(component=ConfigScroll,systems=[scroll_config::system()])]
pub(super) fn config_scroll() {}

#[compose]
pub(super) fn panel(state: model!(Editor), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            Row (height: 14, min_width: 0, column_gap: metrics.gap, shrink: 0.0) {
                Text (
                    text: ${ state.identity() },
                    grow: 1.0,
                    min_width: 0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: SECONDARY,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                Text (
                    text: ${ state.scope().label().to_owned() },
                    shrink: 0.0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: ACCENT
                ) [
                    FONT_PAINT,
                ]
            }
            if ${ !state.status().is_empty() } {
                text_block (${ state.status() }, WARNING, BODY_SIZE, Start, 0, 2)
            }
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                clip_children: true
            ) [
                ScrollOffset::default(),
                ScrollConfig {
                    elastic: false,
                    ..Default::default()
                },
                ConfigScroll {
                    state: state.clone(),
                },
            ] {
                walk ${ state.window() } with index by index {
                    row (state, index, metrics)
                }
                if ${ state.section() == Section::Core && state.count() == 0 && state.ready() } {
                    text_block (
                        "此核心未提供可编辑选项",
                        SECONDARY,
                        TITLE_SIZE,
                        Start,
                        0,
                        3
                    )
                }
            } on SizeChanged { state.viewport(event.new.height, metrics.gap, metrics.shadow_height()); }
        }
    }
}

#[compose]
fn row(state: model!(Editor), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${
                state.extent(index, metrics.gap)
                    + Fixed::from_int(metrics.shadow_height() + metrics.gap)
            },
            min_width: 0,
            shrink: 0.0,
            padding: Padding {
                bottom: Dimension::px(metrics.gap),
                ..Default::default()
            }
        ) {
            Row (
                height: ${ state.extent(index, metrics.gap) },
                min_height: metrics.wifi_row_height(),
                min_width: 0,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                align: AlignItems::Center,
                bg_color: ${ if state.selected() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.selected() == index { ACCENT } else { DIVIDER } },
                transform: ${
                    crate::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if state.selected() == index {
                                super::motion::hop(state.age(), 2.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor::Item {
                    index,
                    layout: ActionLayout::Configuration,
                },
            ] {
                View (width: 3, height: 24, shrink: 0.0, bg_color: ACCENT)
                if ${ state.section() == Section::Home } {
                    Image (
                        src: match index {
                            0 => {
                                path!(
                                    M 1 2 H 15 V 12 H 9 V 14 H 12 V 16 H 4 V 14 H 7 V 12 H 1 Z M 3 4 V 10 H
                                    13 V 4 Z
                                )
                            }
                            1 => {
                                path!(
                                    M 4 4 H 12 V 12 H 4 Z M 5 1 H 7 V 4 H 5 Z M 9 1 H 11 V 4 H 9 Z M 5 12 H 7
                                    V 15 H 5 Z M 9 12 H 11 V 15 H 9 Z M 1 5 H 4 V 7 H 1 Z M 1 9 H 4 V 11 H 1
                                    Z M 12 5 H 15 V 7 H 12 Z M 12 9 H 15 V 11 H 12 Z
                                )
                            }
                            2 => {
                                path!(
                                    M 2 4 H 14 V 6 H 16 V 13 H 12 V 11 H 4 V 13 H 0 V 6 H 2 Z M 3 6 V 8 H 1 V
                                    10 H 3 V 12 H 5 V 10 H 7 V 8 H 5 V 6 Z M 11 6 V 8 H 13 V 6 Z M 13 9 V 11
                                    H 15 V 9 Z
                                )
                            }
                            _ => {
                                path!(
                                    M 2 1 H 12 L 15 4 V 15 H 1 V 1 Z M 4 2 V 6 H 11 V 2 Z M 4 9 V 14 H 12 V 9
                                    Z
                                )
                            }
                        },
                        viewbox: 16,
                        width: 24,
                        height: 24,
                        shrink: 0.0,
                        color: ACCENT
                    )
                }
                Column (
                    height: ${ state.extent(index, metrics.gap) - Fixed::from_int(metrics.gap * 2 + 4) },
                    grow: 1.0,
                    min_width: 0,
                    row_gap: 4
                ) {
                    Text (
                        text: ${ state.row(index).label },
                        width: Dimension::percent(100),
                        height: Dimension::Content,
                        min_height: 26,
                        min_width: 0,
                        font: font(TITLE_SIZE),
                        font_size: TITLE_SIZE,
                        text_color: FOREGROUND,
                        paragraph: super::widgets::paragraph(Start, 2)
                    ) [
                        FONT_PAINT,
                    ] on SizeChanged { state.text_size(index, 0, event.new.height); }
                    if ${ !state.row(index).value.is_empty() } {
                        Text (
                            text: ${ state.row(index).value },
                            width: Dimension::percent(100),
                            height: Dimension::Content,
                            min_width: 0,
                            min_height: 14,
                            font: font(BODY_SIZE),
                            font_size: BODY_SIZE,
                            text_color: SECONDARY,
                            paragraph: crate::ui::widgets::ParagraphStyle {
                                wrap: crate::ui::widgets::TextWrap::Grapheme,
                                max_lines: None,
                                ..Default::default()
                            }
                        ) [
                            FONT_PAINT,
                        ] on SizeChanged { state.text_size(index, 1, event.new.height); }
                    }
                }
                if ${ state.row(index).changed } {
                    View (width: 6, height: 6, bg_color: WARNING, shrink: 0.0)
                }
                if ${ state.row(index).editable && state.section() != Section::Sources } {
                    Text (
                        text: ${
                            if matches!(state.section(), Section::Home | Section::Confirm | Section::Conflict) {
                                ">"
                            } else {
                                "< >"
                            }
                                .to_owned()
                        },
                        font: font(BODY_SIZE),
                        font_size: BODY_SIZE,
                        text_color: ACCENT,
                        shrink: 0.0
                    ) [
                        FONT_PAINT,
                    ]
                }
            }
            shadow (metrics)
        } on SizeChanged { state.measure(index, event.new.height); }
    }
}

#[compose]
pub(super) fn hints(state: model!(Editor), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
            if ${ state.confirmable() } {
                control_key ("A", metrics, true)
                control_label (${ state.confirm_label() }, metrics)
            }
            control_key ("B", metrics, false)
            control_label ("返回", metrics)
            View (grow: 1.0, min_width: 0)
            if ${
                (state.dirty() || !state.ready() && !state.loading())
                    && !matches!(state.section(), Section::Confirm | Section::Conflict)
            } {
                control_key ("X", metrics, false)
                control_label (${ if state.ready() { "保存" } else { "重试" } }, metrics)
            }
            if ${
                state.ready()
                    && !matches!(
                        state.section(), Section::Sources | Section::Confirm | Section::Conflict
                    )
            } {
                control_key ("Y", metrics, false)
                control_label ("范围", metrics)
            }
        }
    }
}
