use super::chrome::{label, live_label};
use super::data::{Phase, Snapshot};
use super::model::{DisplayState, TextField};
use super::style::*;
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::Center;
use TextField::{Headline, HostButton, Next, Peer};
use alloc::{borrow::ToOwned, format, string::ToString, vec::Vec};
fn host_action_enabled(view: &Snapshot) -> bool {
    (view.ready || view.connected || view.phase == Phase::ApprovalPending) && !view.busy
}

#[compose]
fn avatar(number: &str, ready: bool, metrics: Metrics) -> Entity {
    ui! {
        Text (
            number.to_owned(),
            width: if metrics.compact { 36 } else { 60 },
            height: if metrics.compact { 36 } else { 60 },
            bg_color: if ready { ACCENT_DIM } else { PANEL },
            font: font(metrics.title),
            font_size: metrics.title,
            text_color: if ready { ACCENT } else { DISABLED },
            paragraph: paragraph(Center, 1)
        ) [
            StaticCache,
            OffscreenRender::default(),
        ]
    }
}

#[compose]
pub(super) fn host(
    view: &Snapshot,
    state: model!(DisplayState),
    metrics: Metrics,
    frame: Entity,
) -> Entity {
    let enabled = host_action_enabled(view);
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) {
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
                    width: if metrics.compact { 40 } else { 70 },
                    justify: JustifyContent::SpaceEvenly,
                    align: AlignItems::Center
                ) {
                    walk 0..3 with _index {
                        View (
                            width: 5,
                            height: 5,
                            bg_color: if view.ready { ACCENT } else { DISABLED }
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
                        view.ready,
                        metrics
                    )
                    live_label (
                        state,
                        Peer,
                        metrics.body,
                        if view.ready { FOREGROUND } else { SECONDARY },
                        Center
                    )
                }
            }
            Column (
                height: if metrics.compact { 34 } else { 50 },
                shrink: 0.0,
                bg_color: if enabled { ACCENT } else { PANEL },
                border_color: ACCENT,
                border_width: if enabled { 2 } else { 0 },
                justify: JustifyContent::Center
            ) [
                Focusable,
            ] {
                live_label (
                    state,
                    HostButton,
                    metrics.title,
                    if enabled { BACKGROUND } else { DISABLED },
                    Center
                )
            } on SizeChanged(super::input::place::callback(frame, state, 0)) on HoverChanged(super::input::hover::callback(frame, state, 0)) on Tap { state.confirm(); }
        }
    }
}

#[compose]
pub(super) fn network_panel(
    view: &Snapshot,
    state: model!(DisplayState),
    metrics: Metrics,
    frame: Entity,
) -> Entity {
    let enabled = host_action_enabled(view);
    let local = if view.local_address.is_empty() {
        "等待分配地址".into()
    } else {
        format!("本机  {}", view.local_address)
    };
    let peers = view
        .peers
        .iter()
        .map(|peer| format!("{}  {}", peer.name, peer.address))
        .collect::<Vec<_>>()
        .join("\n");
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
                label (&local, metrics.body, SECONDARY, Center)
                Text (
                    peers,
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
                height: if metrics.compact { 34 } else { 50 },
                shrink: 0.0,
                bg_color: if enabled { ACCENT } else { PANEL },
                justify: JustifyContent::Center
            ) [
                Focusable,
            ] {
                live_label (
                    state,
                    HostButton,
                    metrics.title,
                    if enabled { BACKGROUND } else { DISABLED },
                    Center
                )
            } on SizeChanged(super::input::place::callback(frame, state, 0)) on HoverChanged(super::input::hover::callback(frame, state, 0)) on Tap { state.confirm(); }
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
                height: if metrics.compact { 44 } else { 60 },
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
                            width: if metrics.compact { 22 } else { 28 },
                            height: if metrics.compact { 22 } else { 28 },
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
