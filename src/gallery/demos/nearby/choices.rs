use super::chrome::label;
use super::data::{Page, Snapshot};
use super::input::SelectionTarget;
use super::model::DisplayState;
use super::style::*;
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::{Center, Start};
#[compose]
pub(super) fn choice(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let cards = if view.page == Page::Mode {
        [
            (0, "仅组网", "连接附近的掌机", "返回系统后保持连接"),
            (1, "选择游戏", "选择要一起玩的游戏", "创建或加入游戏房间"),
        ]
    } else if view.network_only {
        [
            (0, "创建房间", "邀请好友连接你的掌机", "你是房主"),
            (1, "加入房间", "寻找附近好友的房间", "建立局域网连接"),
        ]
    } else {
        [
            (0, "创建房间", "邀请好友加入你的房间", "你是房主"),
            (1, "加入房间", "寻找附近好友的房间", "一起开始游戏"),
        ]
    };
    ui! {
        Row (
            grow: 1.0,
            min_height: 0,
            column_gap: metrics.gap + 6
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            walk cards.into_iter() with card {
                Column (
                    grow: 1.0,
                    min_width: 0,
                    padding: Padding::all(metrics.inset),
                    row_gap: metrics.gap,
                    bg_color: PANEL,
                    border_radius: 0,
                    clip_children: true
                ) [
                    Focusable,
                    SelectionTarget {
                        frame: None,
                        index: card.0,
                        state: state.clone(),
                    },
                ] {
                    Row (
                        height: if metrics.compact { 28 } else { 44 },
                        align: AlignItems::Center
                    ) {
                        Text (
                            if card.0 == 0 { "+" } else { "2" },
                            width: if metrics.compact { 28 } else { 44 },
                            height: Dimension::percent(100),
                            bg_color: ACCENT_DIM,
                            font: font(metrics.title),
                            font_size: metrics.title,
                            text_color: ACCENT,
                            paragraph: paragraph(Center, 1)
                        )
                    }
                    View (
                        grow: 1.0,
                        min_height: 0
                    )
                    label (
                        card.1,
                        metrics.title,
                        FOREGROUND,
                        Start
                    )
                    Text (
                        card.2,
                        width: Dimension::percent(100),
                        min_width: 0,
                        height: Dimension::Content,
                        font: font(metrics.small),
                        font_size: metrics.small,
                        text_color: SECONDARY,
                        paragraph: paragraph(Start, 2)
                    )
                    if !metrics.compact {
                        label (
                            card.3,
                            metrics.small,
                            SECONDARY,
                            Start
                        )
                    }
                } on SizeChanged(super::input::place::callback(state, card.0)) on HoverChanged(super::input::hover::callback(state, card.0)) on Tap { state.activate(card.0); }
            }
        }
    }
}

#[compose]
fn room_signal(strength: u8, available: bool) -> Entity {
    ui! {
        Row (
            width: 30,
            height: 24,
            shrink: 0.0,
            column_gap: 3,
            align: AlignItems::FlexEnd
        ) {
            walk 0..4 with index {
                View (
                    width: 4,
                    height: 6 + index * 4,
                    bg_color: if available && strength as i32 > index * 25 { ACCENT } else { DIVIDER }
                )
            }
        }
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
                        max_height: if metrics.compact { 42 } else { 70 },
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
                        max_height: if metrics.compact { 42 } else { 70 },
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
                            width: if metrics.compact { 20 } else { 32 },
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
