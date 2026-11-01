use super::actions::overlay;
use super::actor::actor;
use super::catalog::catalog;
use super::chrome::{footer, header};
use super::composition::room_pages;
use super::deck::{FULL, catalog_share, shelf_share, shelf_transform, transform};
use super::launcher::{Launcher, MenuAction, Overlay, PLATFORMS, Screen};
use super::library::favorites;
use super::model::DisplayState;
use super::pixel::{Scenery::Margins, backdrop};
use super::snapshot::frozen_backdrop;
use super::style::*;
use super::widgets::shadow;
use crate::prelude::*;
use crate::ui::widgets::page_view::{CreatePage, PageView};
use Screen::{Favorites, Search};
use alloc::vec::Vec;

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
    if library.screen() == Screen::Search && library.keyboard() {
        library.type_key(super::catalog::KEYS[library.selected().saturating_sub(100).min(39)]);
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

pub(super) fn move_selection(state: &super::model::DisplayStateHandle, delta: isize) {
    let library = state.launcher();
    if library.menu() {
        let current = library.menu_selection();
        library.select_action(current.saturating_add_signed(delta));
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
        let index = current.saturating_add_signed(delta);
        let next = keys().nth(index).or_else(|| keys().next_back());
        if let Some(key) = next {
            library.select(key);
        }
    } else {
        let limit = PLATFORMS.len();
        let current = library.selected();
        library.select(current.saturating_add_signed(delta).min(limit - 1));
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
        .find(|&key| {
            key as usize == library.selected()
                && library
                    .games()
                    .iter()
                    .any(|game| game.key() == key && !game.directory())
        });
    if let Some(key) = key {
        library.begin_hold(key);
    }
}

pub(super) fn finish_play(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    library.release_hold();
    deliver_pending(state);
}

pub(super) fn deliver_pending(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    let Some((key, multiplayer)) = library.pending_play() else {
        return;
    };
    library.acknowledge_play();
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
    let panel_metrics = Metrics {
        compact: true,
        gap: metrics.gap / 2,
        ..metrics
    };
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, bg_color: BACKGROUND, container: true) {
            background (state, library, deck, metrics)
            Column (
                id: "nearby_room_scrim", visible: ${ deck.panel() > Fixed::ZERO },
                position: Position::Absolute, left: 0, top: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                bg_color: ${ Color::rgba(7, 16, 12, (deck.panel() * Fixed::from_int(185)).to_int() as u8) },
                padding: metrics.modal_padding(), justify: JustifyContent::Center, align: AlignItems::Stretch
            ) {
                Column (
                    id: "nearby_room_panel", grow: 1.0, min_height: 0, border_width: 2, border_color: DIVIDER, bg_color: BACKGROUND,
                    transform: ${ crate::types::Transform::translate(Fixed::ZERO, (Fixed::ONE - deck.panel()) * Fixed::from_int(80)).compose(&crate::types::Transform::scale(Fixed::from_ratio(15, 16) + deck.panel() * Fixed::from_ratio(1, 16), Fixed::from_ratio(15, 16) + deck.panel() * Fixed::from_ratio(1, 16))) }
                ) { room_pages (state, panel_metrics) }
                shadow (metrics)
            }
            overlay (state, library)
            actor (state, true)
        }
    }
}

#[compose]
fn page(screen: Screen, state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            match screen {
                Screen :: Favorites => { favorites (state, library) }
                Screen :: Platforms | Screen :: Games | Screen :: Search => { catalog (screen, state, library) }
            }
        }
    }
}

#[compose]
fn background(
    state: model!(DisplayState),
    library: model!(Launcher),
    deck: model!(super::deck::Deck),
    metrics: Metrics,
) -> Entity {
    let mut view = state.snapshot();
    view.page = super::data::Page::Games;
    let source = ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, bg_color: BACKGROUND) [crate::ui::OffscreenRender::default()] {
            header (&view, state, metrics, true)
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
                footer (&view, state, metrics, true)
            }

        }
    };
    ui! { frozen_backdrop (source, state) }
}
