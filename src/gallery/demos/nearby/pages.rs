use super::choices::{choice, games, rooms};
use super::chrome::{footer, header};
use super::data::{Page, Snapshot};
use super::model::DisplayState;
use super::pixel::{Scenery::Margins, backdrop};
use super::sessions::{host, joined, leaving, network_panel, outcome, unavailable};
use super::style::*;
use super::widgets::paragraph;
use crate::prelude::*;
use crate::ui::widgets::{Text, TextAlign::Start};
use alloc::string::{String, ToString};
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
            border_width: 0,
            position: Position::Absolute,
            width: 0,
            height: 0
        ) [
            crate::ui::IgnoreHitTest,
            super::input::SelectionFrame {
                body,
                page: state.page_key(),
            },
        ] {
            View (
                position: Position::Absolute,
                left: ${ Dimension::Percent(-state.motion().expansion().0) },
                width: ${ Dimension::Percent(Fixed::from_int(100) + state.motion().expansion().0 * 2) },
                top: ${ Dimension::Percent(-state.motion().expansion().1) },
                height: ${ Dimension::Percent(Fixed::from_int(100) + state.motion().expansion().1 * 2) },
                border_color: ACCENT,
                border_width: 2
            ) [
                crate::ui::IgnoreHitTest,
            ]
        }
    }
}

#[compose]
fn contents(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let view = view.clone();
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: BACKGROUND
        ) {
            header (
                &view,
                state,
                metrics
            )
            View (
                height: metrics.rule_height(),
                shrink: 0.0,
                bg_color: WARNING
            )
            Column (
                grow: 1.0,
                min_height: 0,
                padding: metrics.page_padding(),
                row_gap: metrics.gap
            ) {
                backdrop (Margins)
                Column (
                    grow: 1.0,
                    min_height: 0,
                    row_gap: if view.page == Page::Lobby { 0 } else { metrics.gap }
                ) {
                    if ${ view.page != Page::Games } {
                        Row (
                            height: if view.page == Page::Lobby { 0 } else { i32::from(metrics.title) + 8 },
                            shrink: 0.0,
                            align: AlignItems::Center,
                            column_gap: metrics.gap
                        ) {
                            Text (
                                text: ${ if view.page == Page::Lobby { String::new() } else { state.heading() } },
                                grow: 1.0,
                                min_width: 0,
                                height: Dimension::Content,
                                min_height: if view.page == Page::Lobby { 0 } else { i32::from(metrics.title) + 8 },
                                font: font(metrics.title),
                                font_size: metrics.title,
                                text_color: FOREGROUND,
                                paragraph: paragraph(Start, 1)
                            ) [
                                FONT_PAINT,
                            ]
                            Row (
                                width: if view.page == Page::Lobby { 0 } else { 36 },
                                clip_children: true,
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
                    }
                    if ${ !(view.page == Page::Games && state.status().is_empty()) } {
                        Text (
                            text: ${ if view.page == Page::Lobby { String::new() } else { state.status().to_string() } },
                            height: if view.page == Page::Lobby { Dimension::px(0) } else { Dimension::Content },
                            min_height: if view.page == Page::Lobby { 0 } else { i32::from(metrics.small) + LABEL_LEADING },
                            shrink: 0.0,
                            width: Dimension::percent(100),
                            min_width: 0,
                            font: font(metrics.small),
                            font_size: metrics.small,
                            text_color: SECONDARY,
                            paragraph: paragraph(Start, 2),
                            clip_children: true
                        ) [
                            FONT_PAINT,
                        ]
                    }
                    if view.network_only && matches!(view.page, Page::Host | Page::Joined) {
                        network_panel (state, metrics)
                    } else {
                        match view.page {
                            Page :: Lobby => {
                                rooms (&view, state, metrics)
                            }
                            Page :: Candidates => {
                                games (&view, state, metrics)
                            }
                            Page :: Choice | Page :: Mode => {
                                choice (&view, state, metrics)
                            }
                            Page :: Host => {
                                host (state, metrics)
                            }
                            Page :: Rooms => {
                                rooms (&view, state, metrics)
                            }
                            Page :: Joined => {
                                joined (state, metrics)
                            }
                            Page :: Leaving => {
                                leaving (state, metrics)
                            }
                            Page :: Unavailable => {
                                unavailable (metrics)
                            }
                            Page :: Outcome => {
                                outcome (&view, metrics)
                            }
                            Page :: Games => {
                                games (&view, state, metrics)
                            }
                        }
                    }
                }
                footer (
                    &view,
                    state,
                    metrics
                )
            }
        }
    }
}
