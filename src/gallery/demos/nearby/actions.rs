use super::consoles::{Platform, console};
use super::launcher::{Launcher, PLATFORMS};
use super::model::DisplayState;
use super::style::*;
use crate::prelude::*;
use crate::ui::widgets::{Image, Text, TextAlign};
use TextAlign::{Center, Start};

const GAP: i32 = 8;
const ITEMS: [&str; 6] = [
    "开始游戏",
    "附近联机",
    "收藏 / 取消收藏",
    "游戏信息",
    "游戏说明",
    "返回",
];

#[component]
pub(super) struct ActionAnchor(pub usize);

#[event]
fn hover(
    node: entity!(),
    event: event!(crate::input::event::hover::HoverChanged),
    anchors: query!(ActionAnchor),
    state: model!(Launcher),
) {
    if event.hovered
        && let Some(anchor) = anchors.get(node)
    {
        state.select_action(anchor.0);
    }
}

#[compose]
pub(super) fn overlay(display: model!(DisplayState), state: model!(Launcher)) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            crate::ui::IgnoreHitTest,
        ] {
            if ${ state.menu() } {
                Column (
                    id: "nearby_actions_scrim",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: Padding {
                        top: Dimension::Percent(Fixed::from_ratio(20, 3)),
                        bottom: Dimension::Percent(Fixed::from_ratio(20, 3)),
                        left: Dimension::Percent(Fixed::from_ratio(15, 4)),
                        right: Dimension::Percent(Fixed::from_ratio(15, 4)),
                    },
                    justify: JustifyContent::Center,
                    bg_color: Color::rgba(7, 16, 12, 185)
                ) {
                    Column (
                        id: "nearby_actions_panel",
                        grow: 1.0,
                        min_height: 0,
                        padding: Padding::all(GAP * 2),
                        row_gap: GAP,
                        bg_color: BACKGROUND,
                        border_width: 2,
                        border_color: DIVIDER
                    ) {
                        label ("游戏操作", 24, FOREGROUND, Start)
                        View (height: 3, bg_color: WARNING, shrink: 0.0)
                        Row (grow: 1.0, min_height: 0, column_gap: GAP * 2) {
                            Column (width: Dimension::percent(38), shrink: 0.0, min_height: 0) {
                                preview (state)
                                shadow ()
                            }
                            Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: GAP) {
                                walk 0_usize..3 with line {
                                    Row (grow: 1.0, min_height: 0, column_gap: GAP) {
                                        walk 0_usize..2 with column {
                                            action_tile (display, state, line * 2 + column)
                                        }
                                    }
                                }
                            }
                        }
                        Row (height: 22, column_gap: GAP * 2, align: AlignItems::Center) {
                            control_key ("A", true)
                            label ("确认", 12, FOREGROUND, Start)
                            control_key ("B", false)
                            label ("返回", 12, FOREGROUND, Start)
                            Column (grow: 1.0, min_width: 0) {
                                label ("", 12, SECONDARY, Start)
                            }
                        }
                    }
                    shadow ()
                }
            }
        }
    }
}

#[compose]
fn preview(state: model!(Launcher)) -> Entity {
    let platform = state
        .games()
        .iter()
        .find(|game| Some(game.key()) == state.game_key())
        .map_or(Platform::Unknown, |game| {
            Platform::from_id(PLATFORMS[game.platform()].1)
        });
    ui! {
        Column (
            id: "nearby_actions_preview",
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(2),
            justify: JustifyContent::Center,
            row_gap: GAP
        ) {
            console (platform)
        }
    }
}

#[compose]
fn action_tile(display: model!(DisplayState), state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                padding: Padding::all(GAP),
                row_gap: GAP / 2,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                bg_color: ${ if state.menu_selection() == index { ACCENT } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == index { WARNING } else { DIVIDER } }
            ) [
                ActionAnchor(index),
            ] {
                action_icon (state, index)
                Text (
                    ITEMS[index],
                    width: Dimension::percent(100),
                    height: Dimension::Content,
                    shrink: 0.0,
                    min_width: 0,
                    font: FONT,
                    font_size: 12,
                    text_color: ${ if state.menu_selection() == index { BACKGROUND } else { FOREGROUND } },
                    paragraph: paragraph(Center, 2)
                )
            } on HoverChanged(hover::callback(state.clone())) on Tap {
                state.select_action(index);
                super::launcher_view::confirm(&display);
            }
            shadow ()
        }
    }
}

#[compose]
fn action_icon(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 16,
            max_height: 32,
            width: Dimension::percent(100),
            align: AlignItems::Center
        ) {
            Column (width: 32, grow: 1.0, min_height: 0) {
                walk ${
                    core::iter::once((
                        index,
                        if state.menu_selection() == index { BACKGROUND } else { FOREGROUND },
                    ))
                } with icon by icon {
                    Image (
                        src: match index {
                            0 => path!(M 3 2 H 6 V 4 H 10 V 7 H 14 V 10 H 10 V 13 H 6 V 15 H 3 Z),
                            1 => {
                                path!(
                                    M 7 1 H 9 V 10 H 7 Z M 4 4 H 6 V 12 H 4 Z M 10 4 H 12 V 12 H 10 Z M 1 7 H
                                    3 V 14 H 1 Z M 13 7 H 15 V 14 H 13 Z
                                )
                            }
                            2 => {
                                path!(
                                    M 6 0 H 10 V 4 H 16 V 7 H 12 V 11 H 14 V 16 H 10 V 13 H 6 V 16 H 2 V 11 H
                                    4 V 7 H 0 V 4 H 6 Z
                                )
                            }
                            5 => {
                                path!(
                                    M 6 2 H 8 V 6 H 15 V 10 H 8 V 14 H 6 V 12 H 4 V 10 H 2 V 6 H 4 V 4 H 6 Z
                                )
                            }
                            _ => {
                                path!(
                                    M 5 1 H 11 V 3 H 13 V 5 H 15 V 11 H 13 V 13 H 11 V 15 H 5 V 13 H 3 V 11 H
                                    1 V 5 H 3 V 3 H 5 Z M 6 5 V 6 H 5 V 10 H 6 V 11 H 10 V 10 H 11 V 6 H 10 V
                                    5 Z
                                )
                            }
                        },
                        viewbox: 16,
                        width: 32,
                        height: Dimension::percent(100),
                        color: icon.1
                    )
                }
            }
        }
    }
}

#[compose]
fn label(text: &'static str, size: u16, color: Color, align: TextAlign) -> Entity {
    ui! {
        Text (
            text,
            height: Dimension::Content,
            min_width: 0,
            shrink: 0.0,
            font: font(size),
            font_size: size,
            text_color: color,
            paragraph: paragraph(align, 2)
        )
    }
}

#[compose]
fn control_key(key: &'static str, primary: bool) -> Entity {
    ui! {
        Text (
            key,
            width: 18,
            height: 18,
            font: FONT,
            font_size: 12,
            bg_color: if primary { ACCENT } else { PANEL },
            text_color: if primary { BACKGROUND } else { FOREGROUND },
            paragraph: paragraph(Center, 1)
        )
    }
}

#[compose]
fn shadow() -> Entity {
    ui! {
        View (height: 4, shrink: 0.0, bg_color: DIVIDER)
    }
}
