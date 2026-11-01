use super::actions::overlay;
use super::actor::actor;
use super::chrome::{footer, header};
use super::consoles::{Platform, console};
use super::deck::{FULL, catalog_share, shelf_share, shelf_transform, transform};
use super::launcher::{Game, Launcher, MenuAction, Overlay, PLATFORMS, Screen};
use super::library::favorites;
use super::model::DisplayState;
use super::pixel::{Scenery::Margins, backdrop};
use super::style::*;
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{
    Text,
    page_view::{CreatePage, PageView},
};
use Screen::{Favorites, Search};
use alloc::{format, vec::Vec};

pub(super) fn confirm(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    if library.menu() {
        if let Some(item) = library.item(library.menu_selection()) {
            if item.enabled {
                activate_action(state, item.action);
            } else {
                library.result(item.reason);
            }
        }
        return;
    }
    match library.screen() {
        Screen::Platforms => library.choose_platform(library.selected().min(PLATFORMS.len() - 1)),
        Screen::Favorites | Screen::Games | Screen::Search => {
            let query = library.query().to_lowercase();
            let keys: Vec<_> = library
                .games()
                .iter()
                .filter(|game| match library.screen() {
                    Screen::Favorites => game.favorite(),
                    Screen::Games => game.platform() == library.platform(),
                    _ => game.matches(&query),
                })
                .map(|game| game.key())
                .collect();
            if let Some(key) = keys
                .iter()
                .find(|&&key| key as usize == library.selected())
                .or(keys.first())
            {
                library.choose_game(*key, *key as usize);
            }
        }
    }
}

pub(super) fn move_selection(state: &super::model::DisplayStateHandle, forward: bool) {
    let library = state.launcher();
    if library.menu() {
        let current = library.menu_selection();
        library.select_action(if forward {
            current.saturating_add(1)
        } else {
            current.saturating_sub(1)
        });
        return;
    }
    let query = library.query().to_lowercase();
    let games = library.games();
    let keys = || {
        games
            .iter()
            .filter(|game| match library.screen() {
                Favorites => game.favorite(),
                Screen::Games => game.platform() == library.platform(),
                _ => game.matches(&query),
            })
            .map(|game| game.key() as usize)
    };
    if matches!(library.screen(), Favorites | Screen::Games | Search) {
        let current = keys()
            .position(|key| key == library.selected())
            .unwrap_or(0);
        let index = if forward {
            current.saturating_add(1)
        } else {
            current.saturating_sub(1)
        };
        let next = keys().nth(index).or_else(|| keys().next_back());
        if let Some(key) = next {
            library.select(key);
        }
    } else {
        let limit = PLATFORMS.len();
        let current = library.selected();
        library.select(if forward {
            current.saturating_add(1).min(limit - 1)
        } else {
            current.saturating_sub(1)
        });
    }
}

pub(super) fn begin_play(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    let query = library.query().to_lowercase();
    let key = library
        .games()
        .iter()
        .filter(|game| match library.screen() {
            Favorites => game.favorite(),
            Screen::Games => game.platform() == library.platform(),
            Search => game.matches(&query),
            _ => false,
        })
        .map(|game| game.key())
        .find(|&key| key as usize == library.selected());
    if let Some(key) = key {
        library.begin_hold(key);
    }
}

pub(super) fn finish_play(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    let Some((key, multiplayer)) = library.held_play() else {
        return;
    };
    library.cancel_hold();
    let Some(game) = library.games().iter().find(|game| game.key() == key) else {
        return;
    };
    if multiplayer {
        state.launch_game(game.name(), game.multiplayer());
    } else {
        library.choose_game(key, key as usize);
        library.launch();
    }
}

fn activate_action(state: &super::model::DisplayStateHandle, action: MenuAction) {
    use MenuAction::*;
    let library = state.launcher();
    match action {
        Play if library.connected() && library.overlay() != Some(Overlay::Network) => {
            library.show_overlay(Overlay::Network)
        }
        Play => library.launch(),
        Multiplayer => join_game(state),
        ToggleFavorite => library.toggle_favorite(),
        ShowControls => library.show_overlay(Overlay::Controls),
        ShowTools => library.show_overlay(Overlay::Tools),
        ShowExit => library.show_overlay(Overlay::Exit),
        ShowPower => library.show_overlay(Overlay::Power),
        ManageNetwork => state.open_network(),
        OpenWifi => library.network(),
        NetworkInfo => library.result(if library.connected() {
            "LOCAL DEMO: Nearby Guest / 192.168.4.2"
        } else {
            "LOCAL DEMO: OFFLINE"
        }),
        SystemInfo => library.result("LOCAL DEMO / MIRUI / 8 GAMES / 6 PLATFORMS"),
        FileManager => library.result("LOCAL DEMO: file manager opened"),
        Exit => library.result("LOCAL DEMO: exit requested"),
        Poweroff => library.result("LOCAL DEMO: power off requested"),
        RestorePlay => {
            if library.connected() {
                library.network();
            }
            library.launch();
        }
        Back => library.back(),
        _ => library.adjust(action),
    }
}

fn join_game(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    let game = library
        .games()
        .iter()
        .find(|game| Some(game.key()) == library.game_key());
    if let Some(game) = game {
        state.launch_game(game.name(), game.multiplayer());
    }
}

fn platform(id: &str) -> Platform {
    Platform::from_id(id)
}

#[event]
fn create_page(event: event!(CreatePage<Screen>), state: model!(DisplayState)) -> Entity {
    ui! {
        page (*event.route(), state.clone(), state.launcher())
    }
}

#[compose]
pub(super) fn launcher(state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! { launcher_deck (state, library, library.deck()) }
}

#[compose]
fn launcher_deck(
    state: model!(DisplayState),
    library: model!(Launcher),
    deck: model!(super::deck::Deck),
) -> Entity {
    let metrics = state.metrics();
    let mut view = state.snapshot();
    view.page = super::data::Page::Games;
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, bg_color: BACKGROUND) {
            header (&view, state, metrics)
            View (height: metrics.rule_height(), shrink: 0.0, bg_color: WARNING)
            Column (grow: 1.0, min_height: 0, padding: metrics.page_padding(), row_gap: metrics.gap) {
                backdrop (Margins)
                Column (id: "nearby_launcher_deck", grow: 1.0, min_height: 0, min_width: 0, container: true) {
                    Column (
                        position: Position::Absolute, width: Dimension::percent(100), top: 0,
                        height: ${ Dimension::Percent(shelf_share(deck.expansion()) * 100) },
                        clip_children: true, min_height: 0, container: true
                    ) {
                        Column (
                            position: Position::Absolute, top: 0, width: Dimension::percent(100),
                            height: @(id("nearby_launcher_deck").height as height) { height },
                            transform: ${ shelf_transform(deck.expansion(), deck.height()) },
                            padding: Padding::all(metrics.gap), min_height: 0
                        ) {
                            favorites (state, library)
                        }
                    }
                    Column (
                        visible: ${ deck.expansion() > Fixed::ZERO },
                        position: Position::Absolute, width: Dimension::percent(100), top: 0,
                        height: ${ Dimension::Percent(catalog_share(deck.expansion()) * 100) },
                        transform: ${ crate::types::Transform::translate(Fixed::ZERO, deck.height() * shelf_share(deck.expansion())) },
                        clip_children: true, min_height: 0, container: true
                    ) {
                        Column (
                            position: Position::Absolute, top: 0, width: Dimension::percent(100),
                            height: @(id("nearby_launcher_deck").height as height) { height * FULL },
                            transform: ${ transform(deck.expansion(), deck.height() * FULL) },
                            padding: Padding::all(metrics.gap), min_height: 0
                        ) {
                            PageView (
                                id: "nearby_launcher_pages", grow: 1.0, min_height: 0, min_width: 0,
                                page: ${ library.catalog_screen() }, transition: ${ library.transition() }
                            ) on CreatePage(create_page::callback(state.clone()))
                        }
                    }
                } on SizeChanged { deck.set_height(event.new.height); }
                footer (&view, state, metrics)
            }
            overlay (state, library)
            actor (state)
        }
    }
}

#[compose]
fn page(screen: Screen, state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            match screen {
                Screen :: Favorites => { favorites (state, library) }
                Screen :: Platforms => { platforms (library) }
                Screen :: Games | Screen :: Search => { games (state, screen, library) }
            }
        }
    }
}

#[compose]
fn platforms(state: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, clip_children: true) [
            ScrollConfig::default(),
            ScrollOffset::default(),
        ] {
            Column (row_gap: 8) {
                walk PLATFORMS.into_iter().enumerate() with item {
                    Row (
                        height: 86,
                        shrink: 0.0,
                        padding: Padding::all(6),
                        column_gap: 12,
                        bg_color: PANEL
                    ) {
                        View (width: 100, shrink: 0.0) {
                            console (platform(item.1.1))
                        }
                        Text (
                            item.1.0,
                            grow: 1.0,
                            font: FONT,
                            font_size: 12,
                            text_color: FOREGROUND
                        )
                        Text (">", width: 24, font: TITLE_FONT, font_size: 24, text_color: ACCENT)
                    } on Tap { state.choose_platform(item.0); }
                }
            }
        }
    }
}

#[compose]
fn games(state: model!(DisplayState), screen: Screen, library: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: 6) {
            if screen == Screen::Search {
                Text (
                    text: ${
                        library.query_revision();
                        format!("{} _", library.query())
                    },
                    height: 26,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: FOREGROUND,
                    bg_color: PANEL
                )
                keyboard (library)
            }
            Column (grow: 1.0, min_height: 0, clip_children: true) [
                ScrollConfig::default(),
                ScrollOffset::default(),
            ] {
                Column (row_gap: 6) {
                    walk ${
                        library.query_revision();
                        let query = library.query().to_lowercase();
                        library
                            .games()
                            .entries()
                            .filter(|(_, game)| {
                                if screen == Screen::Search {
                                    game.matches(&query)
                                } else {
                                    game.platform() == library.platform()
                                }
                            })
                            .collect::<Vec<_>>()
                    } with item by item.0 {
                        game_row (item.1, library)
                    }
                }
            }
        }
    }
}

#[compose]
fn game_row(game: model!(Game), library: model!(Launcher)) -> Entity {
    ui! {
        Row (height: 66, shrink: 0.0, column_gap: 8, padding: Padding::all(6), bg_color: PANEL) {
            View (width: 68, shrink: 0.0) {
                console (platform(PLATFORMS[game.platform()].1))
            }
            Column (grow: 1.0, min_width: 0, justify: JustifyContent::Center) {
                Text (game.name(), font: FONT, font_size: 12, text_color: FOREGROUND)
                Text (
                    PLATFORMS[game.platform()].0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
            }
            Text (
                text: ${ if game.favorite() { "+" } else { ">" } },
                width: 20,
                font: FONT,
                font_size: 12,
                text_color: ACCENT
            )
        } on Tap { library.choose_game(game.key(), game.key() as usize); }
    }
}

#[compose]
fn keyboard(state: model!(Launcher)) -> Entity {
    const KEYS: [&str; 40] = [
        "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K",
        "L", "DEL", "Z", "X", "C", "V", "B", "N", "M", "SPACE", "CLEAR", " ", "1", "2", "3", "4",
        "5", "6", "7", "8", "9", "0",
    ];
    ui! {
        Column (height: 112, shrink: 0.0, row_gap: 2) {
            walk 0..4 with row {
                Row (grow: 1.0, column_gap: 2) {
                    walk KEYS.into_iter().skip(row * 10).take(10) with key {
                        Text (
                            key,
                            grow: 1.0,
                            min_width: 0,
                            font: FONT,
                            font_size: 12,
                            bg_color: PANEL,
                            text_color: FOREGROUND,
                            paragraph: paragraph(crate::ui::widgets::TextAlign::Center, 1)
                        ) on Tap { state.type_key(key); }
                    }
                }
            }
        }
    }
}
