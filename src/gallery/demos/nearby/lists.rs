use super::catalog::search_icon;
use super::choices::choice;
use super::consoles::{Platform, console};
use super::data::{Page, Snapshot};
use super::input::{PaneScope, SelectionTarget};
use super::model::DisplayState;
use super::pixel::{Scenery, Stage::Search, art};
use super::style::*;
use super::widgets::{caption, label, paragraph, shadow, text_block};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::{
    Text,
    TextAlign::{Center, Start},
};
use alloc::string::ToString;

const DECK_PEEK: Fixed = Fixed::from_ratio(14, 100);
const DECK_FULL: Fixed = Fixed::from_ratio(86, 100);
const DECK_MIN_SCALE: Fixed = Fixed::from_ratio(44, 100);

fn sheet_transform(expansion: Fixed, height: Fixed) -> Transform {
    let scale = DECK_MIN_SCALE + (Fixed::ONE - DECK_MIN_SCALE) * expansion;
    Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height * DECK_FULL / 2)
        .compose(&Transform::scale(scale, scale))
}

fn sheet_share(expansion: Fixed) -> Fixed {
    DECK_PEEK + (DECK_FULL - DECK_PEEK) * expansion
}

#[compose]
pub(super) fn lobby(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let mut mode = view.clone();
    mode.page = Page::Mode;
    mode.network_only = false;
    ui! {
        Column (id: "discovery_deck", grow: 1.0, min_height: 0, container: true) on SizeChanged {
            state.set_deck_height(event.new.height);
        } {
            walk [false, true].into_iter() with actions {
                Column (
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    top: 0,
                    height: ${ Dimension::Percent(sheet_share(if actions { state.motion().deck } else { Fixed::ONE - state.motion().deck }) * Fixed::from_int(100)) },
                    transform: ${ Transform::translate(Fixed::ZERO, if actions { state.deck_height() * sheet_share(Fixed::ONE - state.motion().deck) } else { Fixed::ZERO }) },
                    shrink: 0.0,
                    container: true,
                    min_height: 0,
                    clip_children: true
                ) {
                    Column (
                        position: Position::Absolute,
                        width: Dimension::percent(100),
                        height: @(id("discovery_deck").height as deck_height) { deck_height * DECK_FULL },
                        top: 0,
                        transform: ${ sheet_transform(if actions { state.motion().deck } else { Fixed::ONE - state.motion().deck }, state.deck_height()) },
                        shrink: 0.0,
                        row_gap: metrics.gap,
                        padding: Padding::all(metrics.gap)
                    ) [PaneScope(if actions { Page::Mode } else { Page::Lobby })] {
                        if actions {
                            label ("仅组网 / 选择游戏", metrics.body, SECONDARY, Start)
                            choice (&mode, state, metrics)
                        } else {
                            label ("附近正在玩的伙伴", metrics.title, FOREGROUND, Start)
                            discovery_status (state, metrics)
                            rooms (view, state, metrics)
                        }
                    } on Tap { state.expand_discovery(actions); }
                }
            }
        }
    }
}

#[compose]
fn discovery_status(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        caption (move || state.status().into(), || SECONDARY, metrics.small, Start, LABEL_LEADING)
    }
}

#[compose]
pub(super) fn rooms(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) [Scenery::Ridge] {
            rows (view, state, metrics, true)
        }
    }
}

#[compose]
pub(super) fn games(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let platform = Platform::from_id("nes");
    ui! {
        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) [Scenery::Ridge] {
            Column (grow: 46.0, min_width: 0, min_height: 0) {
                rows (view, state, metrics, false)
            }
            Column (grow: 54.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
                Column (grow: 1.0, min_height: 0, bg_color: PANEL, container: true, clip_children: true) {
                    console (platform)
                }
                preview_title (state, metrics)
            }
        }
    }
}

#[compose]
fn preview_title(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Text (
            text: ${ state.selected_title() },
            width: Dimension::percent(100),
            height: metrics.preview_title_height(),
            min_width: 0,
            shrink: 0.0,
            font: font(metrics.title),
            font_size: metrics.title,
            text_color: FOREGROUND,
            bg_color: BACKGROUND,
            paragraph: paragraph(Center, 2)
        ) [FONT_PAINT]
    }
}

#[compose]
pub(super) fn rows(
    view: &Snapshot,
    state: model!(DisplayState),
    metrics: Metrics,
    has_signal: bool,
) -> Entity {
    let view = view.clone();
    let total = if has_signal {
        view.rooms.len()
    } else {
        view.entries.len()
    };
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            if ${ if has_signal { !state.rooms_visible() || total == 0 } else { state.presentation().entries_empty } } {
                Column (grow: 1.0, min_height: 0) {
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        bg_color: PANEL,
                        padding: Padding::all(metrics.inset),
                        justify: JustifyContent::Center,
                        row_gap: metrics.gap
                    ) {
                        if has_signal {
                            art (state, Search, if metrics.compact { 48 } else { 88 })
                            label ("正在搜索附近的房间", metrics.title, FOREGROUND, Center)
                            Text (
                                "请好友先创建房间，并把两台掌机放近一些",
                                width: Dimension::percent(100),
                                height: Dimension::Content,
                                font: font(metrics.small),
                                font_size: metrics.small,
                                text_color: SECONDARY,
                                paragraph: paragraph(Center, 2)
                            ) [FONT_PAINT]
                        } else {
                            Row (height: 48, width: 96, shrink: 0.0, column_gap: metrics.gap, align: AlignItems::Center, justify: JustifyContent::Center) {
                                search_icon (32)
                                View (width: 32, height: 48, shrink: 0.0) [super::pixel::ActorAnchor(Search)]
                            }
                            label ("没有匹配的游戏", metrics.body, SECONDARY, Center)
                        }
                    }
                    shadow (metrics)
                }
            } else {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk ${
                        let first = state.selected().saturating_sub(3).min(total.saturating_sub(4));
                        first..(first + 4).min(total)
                    } with index by index {
                        list_row (&view, state, index, metrics, has_signal)
                    }
                }
            }
        }
    }
}

#[compose]
fn list_row(
    view: &Snapshot,
    state: model!(DisplayState),
    index: usize,
    metrics: Metrics,
    has_signal: bool,
) -> Entity {
    let platform = Platform::from_id("nes");
    let (title, detail, available, strength) = if has_signal {
        let row = &view.rooms[index];
        (
            row.name.as_str(),
            row.detail.as_str(),
            row.available,
            row.signal,
        )
    } else {
        let row = &view.entries[index];
        (row.name.as_str(), row.detail.as_str(), true, 0)
    };
    ui! {
        Column (
            grow: 1.0,
            min_height: metrics.row_min_height(),
            max_height: if has_signal { metrics.row_height() } else { metrics.library_row_height() },
            min_width: 0,
            transform: ${ state.card_motion().card_transform(index) }
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                padding: metrics.row_padding(),
                column_gap: metrics.gap,
                bg_color: ${ state.card_motion().card_color(index) },
                align: AlignItems::Center
            ) [SelectionTarget {
                index,
                scope: has_signal.then_some(Page::Rooms),
            }] {
                View (
                    width: metrics.rail_width(),
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${ if available && state.selected() == index { ACCENT } else { DIVIDER } }
                )
                if !has_signal {
                    View (width: 24, height: 24, shrink: 0.0) {
                        console (platform)
                    }
                }
                Column (grow: 1.0, min_width: 0, justify: JustifyContent::Center) {
                    row_label (title, metrics.row_title, if available { FOREGROUND } else { DISABLED })
                    if has_signal || view.page == Page::Candidates || view.entries[index].directory {
                        row_label (detail, metrics.row_detail, if available { SECONDARY } else { DISABLED })
                    }
                }
                View (width: Dimension::percent(18), min_width: 32, shrink: 0.0)
                if has_signal {
                    View (width: 30, height: if metrics.compact { 26 } else { 30 }, shrink: 0.0) [
                        super::pixel::SignalSprite { state: state.clone(), index, strength, available },
                    ]
                }
            } on HoverChanged(super::input::hover::callback(state, index)) on Tap {
                if has_signal { state.choose_room(index); } else { state.activate(index); }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn row_label(value: &str, size: u16, color: ColorToken) -> Entity {
    let value = value.to_string();
    ui! {
        text_block (move || value.clone(), move || color, size, Start, ROW_LEADING, 1)
    }
}
