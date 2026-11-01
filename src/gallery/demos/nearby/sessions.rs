use super::input::SelectionTarget;
use super::pixel::Stage::Session;
use super::{
    model::{DisplayState, TextField},
    pixel::{self, Scenery, art},
    style::*,
    widgets::{action_label, guidance, label, live_label, paragraph, shadow},
};
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::Text;
use crate::ui::widgets::TextAlign::{Center, End, Start};
use TextField::{Headline, Peer, RoomName};
use alloc::string::ToString;

#[compose]
pub(super) fn host(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) {
            Column (grow: 1.0, min_height: 0) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: Padding::all(metrics.inset),
                    bg_color: PANEL,
                    row_gap: metrics.gap
                ) {
                    View (grow: 1.0, min_height: 56, clip_children: true) [
                        Scenery::Landscape,
                        pixel::SessionSprite {
                            state: state.clone(),
                        },
                    ]
                    Row (height: 22, shrink: 0.0, column_gap: metrics.gap) {
                        Column (grow: 1.0, min_width: 0) {
                            label ("你 · 房主", metrics.body, FOREGROUND, Center)
                        }
                        Column (grow: 1.0, min_width: 0) {
                            live_label (state, Peer, metrics.body, FOREGROUND, Center)
                        }
                    }
                }
                shadow (metrics)
            }
            Column (
                height: metrics.action_height(),
                shrink: 0.0,
                transform: ${ state.card_motion().card_transform(0) }
            ) {
                Column (
                    grow: 1.0,
                    bg_color: ${ if state.action_enabled() { ACCENT } else { PANEL } },
                    border_color: ACCENT,
                    border_width: ${ if state.action_enabled() { 2 } else { 0 } },
                    justify: JustifyContent::Center
                ) [
                    SelectionTarget {
                        scope: None,
                        frame: None,
                        index: 0,
                        state: state.clone(),
                    },
                ] {
                    action_label (state, metrics.title)
                } on SizeChanged(super::input::place::callback(state, 0)) on HoverChanged(super::input::hover::callback(state, 0)) on Tap { state.confirm(); }
                shadow (metrics)
            }
        }
    }
}

#[compose]
pub(super) fn network_panel(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Column (grow: 1.0, min_height: 0) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: Padding::all(metrics.gap),
                    bg_color: PANEL,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    Row (
                        height: if metrics.compact { 24 } else { 32 },
                        shrink: 0.0,
                        align: AlignItems::Center,
                        column_gap: metrics.gap
                    ) {
                        Column (grow: 1.0, min_width: 0) {
                            live_label (state, Headline, metrics.title, FOREGROUND, Start)
                        }
                        View (
                            width: 120,
                            shrink: 0.0,
                            height: Dimension::percent(100),
                            clip_children: true
                        ) [
                            pixel::SessionSprite {
                                state: state.clone(),
                            },
                        ]
                    }
                    Column (grow: 1.0, min_height: 0, justify: JustifyContent::Center) {
                        match ${ state.devices() } {
                            rows => {
                                network_devices (&rows, metrics)
                            }
                        }
                    }
                    guidance (state, metrics)
                }
                shadow (metrics)
            }
            Column (
                height: metrics.action_height(),
                shrink: 0.0,
                transform: ${ state.card_motion().card_transform(0) }
            ) {
                Column (
                    grow: 1.0,
                    bg_color: ${ if state.action_enabled() { ACCENT } else { PANEL } },
                    justify: JustifyContent::Center
                ) [
                    SelectionTarget {
                        scope: None,
                        frame: None,
                        index: 0,
                        state: state.clone(),
                    },
                ] {
                    action_label (state, metrics.title)
                } on SizeChanged(super::input::place::callback(state, 0)) on HoverChanged(super::input::hover::callback(state, 0)) on Tap { state.confirm(); }
                shadow (metrics)
            }
        }
    }
}

#[compose]
fn network_devices(rows: &[super::data::NetworkPeer], metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, justify: JustifyContent::Center, row_gap: metrics.gap) {
            walk rows.iter() with device {
                Row (
                    height: metrics.device_row_height(),
                    shrink: 1.0,
                    min_height: i32::from(metrics.body) + LABEL_LEADING,
                    bg_color: BACKGROUND,
                    align: AlignItems::Center,
                    column_gap: metrics.gap
                ) {
                    View (width: metrics.rail_width(), height: 12, shrink: 0.0, bg_color: ACCENT)
                    Text (
                        device.name.clone(),
                        grow: 1.0,
                        min_width: 0,
                        height: 18,
                        font: font(metrics.body),
                        font_size: metrics.body,
                        text_color: FOREGROUND,
                        paragraph: paragraph(Start, 1),
                        clip_children: true
                    ) [
                        FONT_PAINT,
                    ]
                    Text (
                        device.address.clone(),
                        width: Dimension::Content,
                        shrink: 0.0,
                        height: 18,
                        font: font(metrics.body),
                        font_size: metrics.body,
                        text_color: SECONDARY,
                        paragraph: paragraph(End, 1)
                    ) [
                        FONT_PAINT,
                    ]
                }
            }
        }
    }
}

#[compose]
pub(super) fn joined(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) {
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
                    walk [(0_usize, "连接房间"), (1, "核对游戏"), (2, "等待开局")].into_iter() with step {
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
                            ) [
                                FONT_PAINT,
                            ]
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
                    min_height: if metrics.compact { 32 } else { 64 },
                    clip_children: true
                ) [
                    pixel::SessionSprite {
                        state: state.clone(),
                    },
                ]
                live_label (
                    state,
                    Headline,
                    metrics.title,
                    FOREGROUND,
                    Center
                )
                guidance (state, metrics)
                if ${ !state.room_name().is_empty() } {
                    live_label (state, RoomName, metrics.small, SECONDARY, Center)
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
pub(super) fn leaving(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                bg_color: PANEL,
                padding: Padding::all(metrics.inset),
                row_gap: metrics.gap,
                justify: JustifyContent::Center,
                align: AlignItems::Center
            ) {
                art (state, Session, if metrics.compact { 56 } else { 120 })
                live_label (
                    state,
                    Headline,
                    metrics.title,
                    FOREGROUND,
                    Center
                )
                guidance (state, metrics)
            }
            shadow (metrics)
        }
    }
}

#[compose]
pub(super) fn unavailable(metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) {
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
                ) [
                    FONT_PAINT,
                ]
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
            shadow (metrics)
        }
    }
}

#[compose]
pub(super) fn outcome(view: &super::data::Snapshot, metrics: Metrics) -> Entity {
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
