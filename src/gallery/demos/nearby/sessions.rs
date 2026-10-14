use super::chrome::{label, live_label};
use super::data::Snapshot;
use super::input::SelectionTarget;
use super::model::{DisplayState, TextField};
use super::style::*;
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::Center;
use TextField::{Headline, HostButton, Next, Peer};
use alloc::{borrow::ToOwned, string::ToString};

#[compose]
pub(super) fn outcome(view: &Snapshot, metrics: Metrics) -> Entity {
    use super::data::Outcome;
    let (title, detail) = match view.outcome {
        Outcome::System => ("返回系统", "SYSTEM MENU"),
        Outcome::SinglePlayer => ("单机游玩", "GAME RESUMED / SOLO"),
        Outcome::Multiplayer => ("开始游戏", "GAME STARTED / MULTIPLAYER"),
        Outcome::KeepNetwork => ("保持连接，返回系统", "SYSTEM MENU / NETWORK CONNECTED"),
    };
    ui! {
        Column (
            grow: 1.0,
            bg_color: PANEL,
            justify: JustifyContent::Center,
            row_gap: metrics.gap,
            padding: Padding::all(metrics.inset)
        ) {
            label (title, metrics.title, ACCENT, Center)
            label (&view.title, metrics.body, FOREGROUND, Center)
            label (detail, metrics.small, SECONDARY, Center)
            label ("Restart / Back", metrics.small, SECONDARY, Center)
        }
    }
}
#[compose]
fn avatar(number: &str, local: bool, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Text (
            number.to_owned(),
            width: 60,
            height: 60,
            bg_color: ${ if local || state.presentation().ready { ACCENT_DIM } else { PANEL } },
            font: font(metrics.title),
            font_size: metrics.title,
            text_color: ${ if local || state.presentation().ready { ACCENT } else { DISABLED } },
            paragraph: paragraph(Center, 1)
        ) [
            StaticCache,
            OffscreenRender::default(),
        ]
    }
}

#[compose]
pub(super) fn host(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) {
            live_label (state, Headline, metrics.small, SECONDARY, Center)
            Row (
                grow: 1.0,
                min_height: 0,
                padding: Padding::all(metrics.inset),
                bg_color: PANEL,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                Column (
                    grow: 1.0,
                    min_width: 0,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    avatar (
                        "1",
                        true,
                        state,
                        metrics
                    )
                    label (
                        "你 · 房主",
                        metrics.body,
                        FOREGROUND,
                        Center
                    )
                }
                Row (
                    width: 70,
                    justify: JustifyContent::SpaceEvenly,
                    align: AlignItems::Center
                ) {
                    walk 0..3 with _index {
                        View (
                            width: 5,
                            height: 5,
                            bg_color: ${ if state.presentation().ready { ACCENT } else { DISABLED } }
                        )
                    }
                }
                Column (
                    grow: 1.0,
                    min_width: 0,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    avatar (
                        "2",
                        false,
                        state,
                        metrics
                    )
                    live_label (
                        state,
                        Peer,
                        metrics.body,
                        SECONDARY,
                        Center
                    )
                }
            }
            Column (
                height: 38,
                shrink: 0.0,
                bg_color: ${ if state.action_enabled() { ACCENT } else { PANEL } },
                border_color: ACCENT,
                border_width: ${ if state.action_enabled() { 2 } else { 0 } },
                justify: JustifyContent::Center
            ) [
                Focusable,
                SelectionTarget {
                    frame: None,
                    index: 0,
                    state: state.clone(),
                },
            ] {
                live_label (
                    state,
                    HostButton,
                    metrics.title,
                    BACKGROUND,
                    Center
                )
            } on SizeChanged(super::input::place::callback(state, 0)) on HoverChanged(super::input::hover::callback(state, 0)) on Tap { state.confirm(); }
        }
    }
}

#[compose]
pub(super) fn network_panel(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Column (
                grow: 1.0,
                min_height: 0,
                padding: Padding::all(metrics.inset),
                bg_color: PANEL,
                justify: JustifyContent::Center,
                row_gap: metrics.gap
            ) {
                live_label (state, Headline, metrics.title, FOREGROUND, Center)
                Text (
                    text: ${ state.local_address() },
                    font: FONT,
                    font_size: metrics.body,
                    text_color: SECONDARY,
                    height: 18,
                    paragraph: paragraph(Center, 1)
                )
                Text (
                    text: ${ state.peers() },
                    width: Dimension::percent(100),
                    min_width: 0,
                    height: Dimension::Content,
                    font: font(metrics.small),
                    font_size: metrics.small,
                    text_color: SECONDARY,
                    paragraph: paragraph(Center, 3),
                    clip_children: true
                )
                live_label (state, Next, metrics.small, SECONDARY, Center)
            }
            Column (
                height: 38,
                shrink: 0.0,
                bg_color: ${ if state.action_enabled() { ACCENT } else { PANEL } },
                justify: JustifyContent::Center
            ) [
                Focusable,
                SelectionTarget {
                    frame: None,
                    index: 0,
                    state: state.clone(),
                },
            ] {
                live_label (
                    state,
                    HostButton,
                    metrics.title,
                    BACKGROUND,
                    Center
                )
            } on SizeChanged(super::input::place::callback(state, 0)) on HoverChanged(super::input::hover::callback(state, 0)) on Tap { state.confirm(); }
        }
    }
}

#[compose]
pub(super) fn joined(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(metrics.inset),
            row_gap: metrics.gap,
            justify: JustifyContent::Center,
            clip_children: true
        ) {
            Row (
                height: 60,
                shrink: 0.0,
                column_gap: metrics.gap
            ) {
                walk [(0, "连接房间"), (1, "核对游戏"), (2, "等待开局")].into_iter() with step {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        align: AlignItems::Center,
                        row_gap: metrics.gap
                    ) {
                        Text (
                            (step.0 + 1).to_string(),
                            width: 28,
                            height: 28,
                            bg_color: ${ state.steps()[step.0].background },
                            font: font(metrics.small),
                            font_size: metrics.small,
                            text_color: ${ state.steps()[step.0].foreground },
                            paragraph: paragraph(Center, 1)
                        )
                        label (
                            step.1,
                            metrics.small,
                            SECONDARY,
                            Center
                        )
                    }
                }
            }
            View (
                grow: 1.0,
                min_height: 0,
                max_height: 24
            )
            live_label (
                state,
                Headline,
                metrics.title,
                FOREGROUND,
                Center
            )
            live_label (
                state,
                Next,
                metrics.small,
                SECONDARY,
                Center
            )
            if !view.room_name.is_empty() {
                label (
                    &view.room_name,
                    metrics.small,
                    SECONDARY,
                    Center
                )
            }
        }
    }
}

#[compose]
pub(super) fn leaving(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(metrics.inset),
            row_gap: metrics.gap,
            justify: JustifyContent::Center,
            align: AlignItems::Center
        ) {
            Text (
                "B",
                width: 44,
                height: 36,
                bg_color: ACCENT_DIM,
                font: font(metrics.title),
                font_size: metrics.title,
                text_color: ACCENT,
                paragraph: paragraph(Center, 1)
            )
            live_label (
                state,
                Headline,
                metrics.title,
                FOREGROUND,
                Center
            )
            live_label (
                state,
                Next,
                metrics.small,
                SECONDARY,
                Center
            )
        }
    }
}

#[compose]
pub(super) fn unavailable(metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(metrics.inset),
            row_gap: metrics.gap,
            justify: JustifyContent::Center,
            align: AlignItems::Center
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            Text (
                "!",
                width: 44,
                height: 40,
                bg_color: WARNING_PANEL,
                font: font(metrics.title),
                font_size: metrics.title,
                text_color: WARNING,
                paragraph: paragraph(Center, 1)
            )
            label (
                "暂时无法进行联机",
                metrics.title,
                FOREGROUND,
                Center
            )
            label (
                "按 B 返回，仍然可以单机游玩",
                metrics.small,
                ACCENT,
                Center
            )
        }
    }
}
