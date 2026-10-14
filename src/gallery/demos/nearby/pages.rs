use super::choices::{choice, games, rooms};
use super::chrome::{footer, header};
use super::data::{Page, Snapshot};
use super::model::DisplayState;
use super::sessions::{host, joined, leaving, network_panel, outcome, unavailable};
use super::style::*;
use crate::prelude::*;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::Start;
use alloc::borrow::ToOwned;

#[compose]
pub(super) fn page(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, bg_color: BACKGROUND, clip_children: true) {
            mounted_contents (view, state, metrics)
        }
    }
}

#[compose]
fn mounted_contents(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let body = ui! {
        contents (view, state, metrics)
    };
    ui! {
        View (
            border_color: ACCENT,
            border_width: 2,
            position: Position::Absolute,
            width: 0,
            height: 0
        ) [
            crate::ui::IgnoreHitTest,
            super::input::SelectionFrame {
                body,
                page: state.page_key(),
            },
        ]
    }
}

#[compose]
fn contents(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let heading = match view.page {
        Page::Mode => "选择连接方式",
        Page::Choice if view.network_only => "和好友建立局域网",
        Page::Choice => "和好友一起，开始游戏",
        Page::Host => {
            if view.room_name.is_empty() {
                "正在创建房间"
            } else {
                &view.room_name
            }
        }
        Page::Rooms => "选择好友的房间",
        Page::Games => &view.title,
        Page::Outcome => "LOCAL DEMO",
        Page::Joined => "加入房间",
        Page::Leaving => "退出联机",
        Page::Unavailable => "附近联机",
    };
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: BACKGROUND,
            padding: Padding::all(16),
            row_gap: metrics.gap
        ) {
            header (
                view,
                state,
                metrics
            )
            View (
                height: 1,
                shrink: 0.0,
                bg_color: DIVIDER
            )
            Column (
                grow: 1.0,
                min_height: 0,
                row_gap: metrics.gap
            ) {
                Row (
                    height: i32::from(metrics.title) + 8,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: metrics.gap
                ) {
                    Text (
                        heading.to_owned(),
                        grow: 1.0,
                        min_width: 0,
                        height: Dimension::Content,
                        min_height: i32::from(metrics.title) + 8,
                        font: font(metrics.title),
                        font_size: metrics.title,
                        text_color: FOREGROUND,
                        paragraph: paragraph(Start, 1)
                    )
                    Row (
                        width: 36,
                        shrink: 0.0,
                        align: AlignItems::Center,
                        justify: JustifyContent::SpaceBetween
                    ) {
                        walk 0_usize..3 with index {
                            View (
                                width: 6,
                                height: 6,
                                bg_color: ${ state.activity()[index] }
                            )
                        }
                    }
                }
                Text (
                    text: ${ state.status() },
                    height: Dimension::Content,
                    min_height: i32::from(metrics.small) + 6,
                    shrink: 0.0,
                    width: Dimension::percent(100),
                    min_width: 0,
                    font: font(metrics.small),
                    font_size: metrics.small,
                    text_color: SECONDARY,
                    paragraph: paragraph(Start, 2),
                    clip_children: true
                )
                if view.network_only && matches!(view.page, Page::Host | Page::Joined) {
                    network_panel (state, metrics)
                } else {
                    match view.page {
                        Page :: Choice | Page :: Mode => {
                            choice (view, state, metrics)
                        }
                        Page :: Host => {
                            host (state, metrics)
                        }
                        Page :: Rooms => {
                            rooms (view, state, metrics)
                        }
                        Page :: Joined => {
                            joined (view, state, metrics)
                        }
                        Page :: Leaving => {
                            leaving (state, metrics)
                        }
                        Page :: Unavailable => {
                            unavailable (metrics)
                        }
                        Page :: Games => {
                            games (view, state, metrics)
                        }
                        Page :: Outcome => {
                            outcome (view, metrics)
                        }
                    }
                }
            }
            View (
                height: 1,
                shrink: 0.0,
                bg_color: DIVIDER
            )
            footer (
                view,
                state,
                metrics
            )
        }
    }
}
