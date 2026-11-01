use super::data::{Page, Snapshot};
use super::input::SelectionTarget;
use super::model::DisplayState;
use super::pixel::Scenery;
use super::pixel::{
    ChoiceSprite,
    Stage::{Search, Walker},
    art,
};
use super::style::*;
use super::widgets::{label, shadow};
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::{Center, Start};
#[compose]
pub(super) fn choice(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let page = view.page;
    let network_only = view.network_only;
    let cards = if page == Page::Mode {
        [
            (0, "仅组网", "连接附近的掌机"),
            (1, "选择游戏", "选择要一起玩的游戏"),
        ]
    } else if network_only {
        [
            (0, "创建房间", "邀请好友连接你的掌机"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    } else {
        [
            (0, "创建房间", "邀请好友加入你的房间"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    };
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (grow: 1.0, min_height: 0, column_gap: metrics.choice_gap()) {
                walk cards.into_iter() with card {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        min_height: 0,
                        transform: ${ state.card_motion().card_transform(card.0) }
                    ) {
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            min_height: 0,
                            padding: metrics.card_padding(),
                            row_gap: metrics.gap,
                            bg_color: ${ state.card_motion().card_color(card.0) }
                        ) [
                            SelectionTarget {
                                frame: None,
                                index: card.0,
                                state: state.clone(),
                            },
                        ] {
                            View (
                                grow: 1.0,
                                min_height: metrics.choice_art_height(),
                                clip_children: true
                            ) [
                                Scenery::Landscape,
                                ChoiceSprite {
                                    state: state.clone(),
                                    index: card.0,
                                    mode: page == Page::Mode,
                                },
                            ] {
                                Text (
                                    if card.0 == 0 { "+" } else { "2" },
                                    position: Position::Absolute,
                                    left: 6,
                                    top: 6,
                                    width: 24,
                                    height: 20,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    bg_color: BACKGROUND,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 1)
                                ) [
                                    crate::ui::dirty::PaintInflate::uniform(Fixed::ONE),
                                ]
                            }
                            label (card.1, metrics.title, FOREGROUND, Center)
                            if !metrics.tiny {
                                Text (
                                    card.2,
                                    width: Dimension::percent(100),
                                    min_width: 0,
                                    height: Dimension::Content,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 2)
                                ) [
                                    crate::ui::dirty::PaintInflate::uniform(Fixed::ONE),
                                ]
                            }
                        } on SizeChanged(super::input::place::callback(state, card.0)) on HoverChanged(super::input::hover::callback(state, card.0)) on Tap { state.activate(card.0); }
                        shadow (metrics)
                    }
                }
            }
            if ${ page == Page::Mode && state.current_game() } {
                Row (
                    height: 36,
                    shrink: 0.0,
                    bg_color: PANEL,
                    padding: Padding::all(6),
                    transform: ${ state.card_motion().card_transform(2) }
                ) [
                    SelectionTarget {
                        frame: None,
                        index: 2,
                        state: state.clone(),
                    },
                ] {
                    label ("+ 创建当前游戏的房间", metrics.body, FOREGROUND, Center)
                }
            }
            art (state, Walker, metrics.walker_height())
        }
    }
}

#[compose]
fn room_signal(state: model!(DisplayState), index: usize, strength: u8, available: bool) -> Entity {
    ui! {
        View (
            width: 30,
            height: 26,
            shrink: 0.0
        ) [
            super::pixel::SignalSprite {
                state: state.clone(),
                index,
                strength,
                available,
            },
        ]
    }
}

#[compose]
pub(super) fn rooms(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let first = view
        .selected
        .saturating_sub(3)
        .min(view.rooms.len().saturating_sub(4));
    let rows = view.rooms.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            if ${ !state.rooms_visible() } {
                Column (
                    grow: 1.0,
                    bg_color: PANEL,
                    padding: Padding::all(metrics.inset),
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) [
                    SelectionTarget {
                        frame: None,
                        index: 0,
                        state: state.clone(),
                    },
                ] {
                    art (state, Search, if metrics.compact { 42 } else { 84 })
                    label (
                        "正在搜索附近的房间",
                        metrics.title,
                        FOREGROUND,
                        Center
                    )
                    Text (
                        "请好友先创建房间，并把两台掌机放近一些",
                        width: Dimension::percent(100),
                        height: Dimension::Content,
                        font: font(metrics.small),
                        font_size: metrics.small,
                        text_color: SECONDARY,
                        paragraph: paragraph(Center, 2)
                    )
                }
            } else {
                walk rows.iter().enumerate().skip(first).take(4) with item {
                    Row (
                        grow: 1.0,
                        min_height: i32::from(metrics.row_title + metrics.row_detail) + 17,
                        max_height: 70,
                        padding: Padding::all(2),
                        column_gap: metrics.gap,
                        bg_color: PANEL,
                        align: AlignItems::Center,
                        clip_children: true
                    ) [
                        Focusable,
                        SelectionTarget {
                            frame: None,
                            index: item.0,
                            state: state.clone(),
                        },
                    ] {
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            justify: JustifyContent::Center
                        ) {
                            label (
                                &item.1.name,
                                metrics.row_title,
                                if item.1.available { FOREGROUND } else { DISABLED },
                                Start
                            )
                            label (
                                if item.1.available {
                                    &item.1.detail
                                } else {
                                    "房间已离线，等待重新出现"
                                },
                                metrics.row_detail,
                                if item.1.available { SECONDARY } else { DISABLED },
                                Start
                            )
                        }
                        room_signal (
                            state,
                            item.0,
                            item.1.signal,
                            item.1.available
                        )
                    } on SizeChanged(super::input::place::callback(state, item.0)) on HoverChanged(super::input::hover::callback(state, item.0)) on Tap { state.activate(item.0); }
                }
                View (
                    grow: 1.0,
                    min_height: 0
                )
            }
        }
    }
}

#[compose]
pub(super) fn games(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            if view.entries.is_empty() {
                Column (
                    grow: 1.0,
                    bg_color: PANEL,
                    justify: JustifyContent::Center,
                    padding: Padding::all(metrics.inset)
                ) {
                    label (
                        "没有可选游戏",
                        metrics.title,
                        FOREGROUND,
                        Center
                    )
                }
            } else {
                walk view.entries.iter().take(4).enumerate() with item {
                    Row (
                        grow: 1.0,
                        min_height: i32::from(metrics.row_title + metrics.row_detail) + 17,
                        max_height: 70,
                        padding: Padding::all(2),
                        column_gap: metrics.gap,
                        bg_color: PANEL,
                        align: AlignItems::Center,
                        clip_children: true
                    ) [
                        Focusable,
                        SelectionTarget {
                            frame: None,
                            index: item.0,
                            state: state.clone(),
                        },
                    ] {
                        Text (
                            if item.1.directory { ">" } else { "·" },
                            width: 32,
                            shrink: 0.0,
                            height: Dimension::Content,
                            font: font(metrics.title),
                            font_size: metrics.title,
                            text_color: ACCENT,
                            paragraph: paragraph(Center, 1)
                        )
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            justify: JustifyContent::Center
                        ) {
                            label (
                                &item.1.name,
                                metrics.row_title,
                                FOREGROUND,
                                Start
                            )
                            label (
                                &item.1.detail,
                                metrics.row_detail,
                                SECONDARY,
                                Start
                            )
                        }
                    } on SizeChanged(super::input::place::callback(state, item.0)) on HoverChanged(super::input::hover::callback(state, item.0)) on Tap { state.activate(item.0); }
                }
                View (
                    grow: 1.0,
                    min_height: 0
                )
            }
        }
    }
}
