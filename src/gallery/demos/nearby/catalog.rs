use super::consoles::{Platform, console};
use super::launcher::Screen::Search;
use super::launcher::{Game, Launcher, PLATFORMS, Screen};
use super::library::{LibraryTarget, focus};
use super::model::DisplayState;
use super::pixel::{ActorAnchor, Scenery, Stage};
use super::style::*;
use super::widgets::{caption, paragraph, shadow};
use crate::prelude::*;
use crate::ui::widgets::{
    Image, Text,
    TextAlign::{Center, Start},
};
use alloc::{format, string::String, vec::Vec};

pub(super) const KEYS: [&str; 40] = [
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K", "L",
    "退格", "Z", "X", "C", "V", "B", "N", "M", "空格", "清空", "结果", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "0",
];

fn first(selected: usize, total: usize) -> usize {
    selected.saturating_sub(3).min(total.saturating_sub(4))
}

#[compose]
pub(super) fn catalog(
    screen: Screen,
    state: model!(DisplayState),
    library: model!(Launcher),
) -> Entity {
    let metrics = state.metrics();
    let body = ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            if screen == Screen::Search {
                search (library, metrics)
            } else {
                games (screen, library, metrics)
            }
        }
    };
    ui! {
        focus (body, library, screen)
    }
}

#[compose]
fn games(screen: Screen, library: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) [
            Scenery::Ridge,
        ] {
            Column (grow: 46.0, min_width: 0, min_height: 0) {
                rows (screen, library, metrics)
            }
            Column (grow: 54.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    bg_color: PANEL,
                    container: true,
                    clip_children: true
                ) {
                    walk ${
                        let platform = if screen == Screen::Platforms {
                            library.selected().min(PLATFORMS.len() - 1)
                        } else {
                            library
                                .games()
                                .iter()
                                .find(|game| game.key() as usize == library.selected())
                                .map_or(library.platform(), |game| game.platform())
                        };
                        core::iter::once(Platform::from_id(PLATFORMS[platform].1))
                    } with platform by platform {
                        console (platform)
                    }
                }
                Text (
                    text: ${
                        if screen == Screen::Platforms {
                            PLATFORMS[library.selected().min(PLATFORMS.len() - 1)].0.into()
                        } else {
                            library
                                .games()
                                .iter()
                                .find(|game| game.key() as usize == library.selected())
                                .map(|game| game.name())
                                .unwrap_or_default()
                        }
                    },
                    width: Dimension::percent(100),
                    height: metrics.preview_title_height(),
                    min_width: 0,
                    shrink: 0.0,
                    font: font(metrics.title),
                    font_size: metrics.title,
                    text_color: FOREGROUND,
                    bg_color: BACKGROUND,
                    paragraph: paragraph(Center, 2)
                ) [
                    FONT_PAINT,
                ]
            }
        }
    }
}

#[compose]
fn rows(screen: Screen, library: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            if screen == Screen::Platforms {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk ${
                        let start = first(library.selected(), PLATFORMS.len());
                        start..(start + 4).min(PLATFORMS.len())
                    } with index by index {
                        entry (library, None, index, screen, metrics)
                    }
                }
            } else {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    if ${
                        library.query_revision();
                        let query = library.query().to_lowercase();
                        library
                            .games()
                            .iter()
                            .all(|game| {
                                if screen == Screen::Search {
                                    !game.matches(&query) || query.trim().is_empty()
                                } else {
                                    game.platform() != library.platform()
                                }
                            })
                    } {
                        empty (library, metrics)
                    } else {
                        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                            walk ${
                                library.query_revision();
                                let query = library.query().to_lowercase();
                                let matches = |game: &super::launcher::GameHandle| {
                                    if screen == Screen::Search {
                                        game.matches(&query)
                                    } else {
                                        game.platform() == library.platform()
                                    }
                                };
                                let games = library.games();
                                let selected = games
                                    .iter()
                                    .filter(matches)
                                    .position(|game| game.key() as usize == library.selected())
                                    .unwrap_or(0);
                                let total = games.iter().filter(matches).count();
                                games
                                    .entries()
                                    .filter(|(_, game)| matches(game))
                                    .skip(first(selected, total))
                                    .take(4)
                                    .collect::<Vec<_>>()
                            } with item by item.0 {
                                entry (
                                    library,
                                    Some(item.1.clone()),
                                    item.1.key() as usize,
                                    screen,
                                    metrics
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

#[compose]
fn entry(
    library: model!(Launcher),
    game: model!(optional Game),
    key: usize,
    screen: Screen,
    metrics: Metrics,
) -> Entity {
    let platform = game.as_ref().map_or(key, |game| game.platform());
    let icon = Platform::from_id(PLATFORMS[platform].1);
    ui! {
        Column (
            grow: 1.0,
            min_height: metrics.row_min_height(),
            max_height: metrics.library_row_height(),
            min_width: 0,
            transform: ${ library.card_motion().card_transform(key) }
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                padding: metrics.row_padding(),
                column_gap: metrics.gap,
                bg_color: ${ library.card_motion().card_color(key) },
                align: AlignItems::Center
            ) [
                LibraryTarget { key, screen },
            ] {
                View (
                    width: metrics.rail_width(),
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${ if library.selected() == key { ACCENT } else { DIVIDER } }
                )
                View (width: 24, height: 24, shrink: 0.0) {
                    console (icon)
                }
                Column (grow: 1.0, min_width: 0, justify: JustifyContent::Center) {
                    Text (
                        text: ${ game.as_ref().map_or_else(|| String::from(PLATFORMS[platform].0), |game| game.name()) },
                        width: Dimension::percent(100),
                        min_width: 0,
                        min_height: i32::from(metrics.row_title) + ROW_LEADING,
                        height: Dimension::Content,
                        font: font(metrics.row_title),
                        font_size: metrics.row_title,
                        text_color: FOREGROUND,
                        paragraph: paragraph(Start, 1),
                        clip_children: true
                    ) [
                        FONT_PAINT,
                    ]
                    if screen == Screen::Platforms || screen == Screen::Search {
                        Text (
                            text: ${
                                if screen == Screen::Platforms {
                                    format!(
                                        "{} 款游戏", library.games().iter().filter(| game | game.platform() ==
                                        platform).count()
                                    )
                                } else {
                                    PLATFORMS[platform].0.into()
                                }
                            },
                            width: Dimension::percent(100),
                            min_width: 0,
                            min_height: i32::from(metrics.row_detail) + ROW_LEADING,
                            height: Dimension::Content,
                            font: font(metrics.row_detail),
                            font_size: metrics.row_detail,
                            text_color: SECONDARY,
                            paragraph: paragraph(Start, 1),
                            clip_children: true
                        ) [
                            FONT_PAINT,
                        ]
                    }
                }
                if ${ game.as_ref().is_some_and(|game| game.favorite()) } {
                    Image (
                        src: path!(
                            M 8 0 H 12 V 4 H 20 V 8 H 16 V 12 H 18 V 20 H 12 V 16 H 8 V 20 H 2 V 12 H 4 V 8 H
                            0 V 4 H 8 Z
                        ),
                        viewbox: 20,
                        width: 12,
                        height: 12,
                        shrink: 0.0,
                        color: WARNING
                    )
                }
                View (width: Dimension::percent(18), min_width: 32, shrink: 0.0)
            } on HoverChanged {
                let hovered = event.hovered;
                if hovered {
                    library.select(key);
                }
            } on Tap {
                if let Some(game) = &game {
                    library.choose_game(game.key(), key);
                } else {
                    library.choose_platform(key);
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn empty(library: model!(Launcher), metrics: Metrics) -> Entity {
    let label = library.clone();
    ui! {
        Column (grow: 1.0, min_height: 0) {
            Column (
                grow: 1.0,
                bg_color: PANEL,
                justify: JustifyContent::Center,
                align: AlignItems::Center,
                row_gap: metrics.gap * 2,
                padding: Padding::all(metrics.inset)
            ) {
                Row (
                    height: 48,
                    width: 96,
                    shrink: 0.0,
                    column_gap: metrics.gap,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center
                ) {
                    search_icon (32)
                    View (width: 32, height: 48, shrink: 0.0) [
                        ActorAnchor(Stage::Search),
                    ]
                }
                caption (
                    move || {
                        if label.query().trim().is_empty() {
                            "输入游戏名或文件名".into()
                        } else {
                            "没有匹配的游戏".into()
                        }
                    },
                    || SECONDARY,
                    metrics.body,
                    Center,
                    LABEL_LEADING
                )
                Row (height: 4, column_gap: metrics.gap, justify: JustifyContent::Center)
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn search(library: model!(Launcher), metrics: Metrics) -> Entity {
    let query = library.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (
                height: i32::from(metrics.body) + LABEL_LEADING + metrics.gap * 2 + 4,
                shrink: 0.0,
                min_width: 0,
                align: AlignItems::Center,
                column_gap: metrics.gap,
                padding: Padding::all(metrics.gap),
                bg_color: PANEL,
                border_width: ${ if library.keyboard() { 2 } else { 1 } },
                border_color: ${ if library.keyboard() { ACCENT } else { DIVIDER } }
            ) {
                search_icon (16)
                Column (grow: 1.0, min_width: 0) {
                    caption (
                        move || {
                            query.query_revision();
                            let value = query.query();
                            if value.is_empty() { "输入游戏名或文件名".into() } else { value }
                        },
                        || FOREGROUND,
                        metrics.body,
                        Start,
                        LABEL_LEADING
                    )
                }
            } on Tap { library.edit_query(); }
            if ${ library.keyboard() } {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk 0_usize..4 with line {
                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) {
                            walk 0_usize..10 with column {
                                keycap (library, line * 10 + column, metrics)
                            }
                        }
                    }
                }
            } else {
                rows (Search, library, metrics)
            }
        }
    }
}

#[compose]
fn keycap(library: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    let text = library.clone();
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${ library.card_motion().card_transform(100 + index) }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                justify: JustifyContent::Center,
                padding: Padding {
                    right: 12.into(),
                    ..Default::default()
                },
                bg_color: ${ if library.selected() == 100 + index { ACCENT } else { PANEL } }
            ) [
                LibraryTarget {
                    key: 100 + index,
                    screen: Screen::Search,
                },
            ] {
                caption (
                    move || KEYS[index].into(),
                    move || if text.selected() == 100 + index { ON_ACCENT } else { FOREGROUND },
                    if KEYS[index].len() == 1 { metrics.title } else { metrics.body },
                    Center,
                    LABEL_LEADING
                )
            } on HoverChanged {
                let hovered = event.hovered;
                if hovered {
                    library.select(100 + index);
                }
            } on Tap { library.type_key(KEYS[index]); }
            shadow (metrics)
        }
    }
}

#[compose]
pub(super) fn search_icon(size: i32) -> Entity {
    ui! {
        Image (
            src: path!(
                M 3 0 H 9 V 2 H 11 V 4 H 13 V 9 H 11 V 11 H 9 V 13 H 4 V 11 H 2 V 9 H 0 V 4 H 2 V
                2 H 3 Z M 4 3 H 3 V 9 H 4 V 10 H 9 V 9 H 10 V 4 H 9 V 3 Z M 11 10 H 13 V 12 H 15
                V 14 H 16 V 16 H 14 V 15 H 12 V 13 H 10 V 11 Z
            ),
            viewbox: 16,
            width: size,
            height: size,
            shrink: 0.0,
            color: ACCENT
        )
    }
}
