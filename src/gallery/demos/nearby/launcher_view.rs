use super::consoles::{Platform, console};
use super::launcher::{Game, Launcher, PLATFORMS, Screen, TOOLS, Tool};
use super::model::DisplayState;
use super::style::*;
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{
    Text,
    page_view::{CreatePage, PageView},
};
use Screen::{Favorites, Platforms, Search, Tools};
use alloc::{format, string::String, vec::Vec};

pub(super) fn confirm(state: &super::model::DisplayStateHandle) {
    let library = state.launcher();
    if library.menu() {
        match library.menu_selection() {
            0 => library.launch(),
            1 => join_game(state),
            2 => library.toggle_favorite(),
            3 => library.show_game_page(Screen::Info),
            4 => library.show_game_page(Screen::Manual),
            _ => library.dismiss_menu(),
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
        Screen::Tools => activate_tool(state, TOOLS[library.selected().min(TOOLS.len() - 1)].0),
        Screen::Wifi => library.network(),
        Screen::Files => library.open(Screen::Platforms),
        _ => library.back(),
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
        let limit = if library.screen() == Tools {
            TOOLS.len()
        } else {
            PLATFORMS.len()
        };
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

fn activate_tool(state: &super::model::DisplayStateHandle, tool: Tool) {
    let library = state.launcher();
    match tool {
        Tool::Wifi => library.tool(Screen::Wifi),
        Tool::Files => library.tool(Screen::Files),
        Tool::Network => state.open_network(),
        Tool::NetworkInfo => library.result(if library.connected() {
            "LOCAL DEMO: Nearby Guest / 192.168.4.2"
        } else {
            "LOCAL DEMO: OFFLINE"
        }),
        Tool::Controls => library.result("A: select / B: back / X: play / hold X: multiplayer"),
        Tool::SystemInfo => library.result("LOCAL DEMO / MIRUI / 8 GAMES / 6 PLATFORMS"),
        Tool::RestorePlay => library.result("LOCAL DEMO: game resumed"),
        Tool::Exit => library.result("LOCAL DEMO: exit requested"),
        Tool::Poweroff => library.result("LOCAL DEMO: power off requested"),
        Tool::Back => library.back(),
        _ => library.adjust_tool(tool),
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
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, bg_color: BACKGROUND) {
            Row (height: 42, shrink: 0.0, padding: Padding::all(8), column_gap: 8) {
                Text (
                    "H",
                    width: 28,
                    bg_color: ACCENT_DIM,
                    text_color: ACCENT,
                    font: TITLE_FONT,
                    font_size: 24
                )
                Text (
                    "NEARBY",
                    grow: 1.0,
                    min_width: 0,
                    font: FONT,
                    font_size: 12,
                    text_color: FOREGROUND
                )
                nav (library, Favorites, "收藏")
                nav (library, Platforms, "平台")
                nav (library, Search, "搜索")
                nav (library, Tools, "设置")
            }
            View (height: 2, shrink: 0.0, bg_color: ACCENT)
            PageView (
                id: "nearby_launcher_pages",
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                page: ${ library.screen() },
                transition: ${ library.transition() }
            ) on CreatePage(create_page::callback(state.clone()))
            actions (state, library)
            Row (height: 30, shrink: 0.0, column_gap: 8, padding: Padding::all(6)) {
                Text (
                    "A 确认  B 返回",
                    grow: 1.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
                Text ("LOCAL DEMO", font: FONT, font_size: 12, text_color: ACCENT)
                Text (
                    text: ${
                        if library.hold_progress() >= Fixed::ONE {
                            "X: LINK!"
                        } else if library.holding() {
                            "X: HOLD..."
                        } else {
                            "X: PLAY"
                        }
                    },
                    font: FONT,
                    font_size: 12,
                    text_color: ACCENT
                ) on Pressed { begin_play(&state); } on Released { finish_play(&state); } on PressCancelled { library.cancel_hold(); }
                Text ("附近联机 >", font: FONT, font_size: 12, text_color: FOREGROUND) on Tap { library.close(); }
            }
        }
    }
}

#[compose]
fn nav(state: model!(Launcher), screen: Screen, title: &'static str) -> Entity {
    ui! {
        Text (
            title,
            min_width: 32,
            font: FONT,
            font_size: 12,
            text_color: ${ if state.screen() == screen { ACCENT } else { SECONDARY } }
        ) on Tap { state.open(screen); }
    }
}

#[compose]
fn page(screen: Screen, state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            min_width: 0,
            padding: Padding::all(12),
            row_gap: 8,
            bg_color: BACKGROUND
        ) {
            Row (height: 32, shrink: 0.0, column_gap: 8) {
                Text (
                    screen.title(),
                    grow: 1.0,
                    min_width: 0,
                    font: TITLE_FONT,
                    font_size: 24,
                    text_color: FOREGROUND
                )
                Text ("< 返回", font: FONT, font_size: 12, text_color: SECONDARY) on Tap { library.back(); }
            }
            match screen {
                Screen :: Favorites => {
                    favorites (state, library)
                }
                Screen :: Platforms => {
                    platforms (library)
                }
                Screen :: Games | Screen :: Search => {
                    games (state, screen, library)
                }
                Screen :: Tools => {
                    tools (state, library)
                }
                Screen :: Wifi => {
                    wifi (library)
                }
                Screen :: Files => {
                    files (library)
                }
                Screen :: Info | Screen :: Manual | Screen :: Result => {
                    detail (library, screen)
                }
            }
        }
    }
}

#[compose]
fn favorites(state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: 6) {
            Column (grow: 1.0, min_height: 0, clip_children: true) [
                ScrollConfig::default(),
                ScrollOffset::default(),
            ] {
                Column (row_gap: 8, padding: Padding::all(4)) {
                    walk ${ 0..library.games().iter().filter(|game| game.favorite()).count().div_ceil(2) } with line {
                        Row (height: 136, shrink: 0.0, column_gap: 8) {
                            walk ${
                                library
                                    .games()
                                    .entries()
                                    .filter(|(_, game)| game.favorite())
                                    .skip(line * 2)
                                    .take(2)
                                    .collect::<Vec<_>>()
                            } with item by item.0 {
                                cartridge (state, item.1, library)
                            }
                        }
                    }
                }
            }
            Text (
                "选择游戏，开始新的旅途",
                height: 20,
                font: FONT,
                font_size: 12,
                text_color: SECONDARY
            )
        }
    }
}

#[compose]
fn cartridge(state: model!(DisplayState), game: model!(Game), library: model!(Launcher)) -> Entity {
    let key = game.key();
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            padding: Padding::all(5),
            row_gap: 4,
            border_width: 2,
            border_color: ${ if library.selected() == key as usize { ACCENT } else { DIVIDER } },
            bg_color: PANEL
        ) {
            View (height: 4, shrink: 0.0, bg_color: DIVIDER)
            console (platform(PLATFORMS[game.platform()].1))
            Text (
                text: ${ game.name() },
                height: 20,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                text_color: FOREGROUND,
                paragraph: paragraph(crate::ui::widgets::TextAlign::Start, 1)
            )
            Row (height: 4, shrink: 0.0, column_gap: 4, justify: JustifyContent::Center) {
                walk 0..8 with _pin {
                    View (width: 4, bg_color: WARNING)
                }
            }
        } on Tap { library.choose_game(key, key as usize); }
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

#[compose]
fn actions(state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            visible: ${ library.menu() }
        ) {
            if ${ library.menu() } {
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: Padding::all(16),
                    justify: JustifyContent::Center,
                    bg_color: Color::rgba(7, 16, 12, 220)
                ) {
                    Column (
                        padding: Padding::all(12),
                        row_gap: 6,
                        bg_color: BACKGROUND,
                        border_width: 2,
                        border_color: ACCENT
                    ) {
                        Text (
                            "游戏操作",
                            height: 30,
                            font: TITLE_FONT,
                            font_size: 24,
                            text_color: FOREGROUND
                        )
                        Text (
                            "开始游戏",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 0 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap { library.launch(); }
                        Text (
                            "附近联机",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 1 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap {
                            if let Some(key) = library.game_key() {
                                if let Some(game) = library.games().iter().find(|game| game.key() == key) {
                                    state.launch_game(game.name(), game.multiplayer());
                                }
                            }
                        }
                        Text (
                            "收藏 / 取消收藏",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 2 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap { library.toggle_favorite(); }
                        Text (
                            "游戏信息",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 3 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap { library.show_game_page(Screen::Info); }
                        Text (
                            "游戏说明",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 4 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap { library.show_game_page(Screen::Manual); }
                        Text (
                            "返回",
                            height: 28,
                            shrink: 0.0,
                            font: FONT,
                            font_size: 12,
                            bg_color: ${ if library.menu_selection() == 5 { ACCENT_DIM } else { PANEL } },
                            text_color: FOREGROUND
                        ) on Tap { library.dismiss_menu(); }
                    }
                }
            }
        }
    }
}

#[compose]
fn tools(state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: 6) {
            Column (grow: 1.0, min_height: 0, clip_children: true) [
                ScrollConfig::default(),
                ScrollOffset::default(),
            ] {
                Column (row_gap: 6) {
                    walk 0..4 with row {
                        Row (height: 64, shrink: 0.0, column_gap: 6) {
                            walk 0..4 with column {
                                tool_tile (state, library, row * 4 + column)
                            }
                        }
                    }
                }
            }
            Text (text: ${ library.notice() }, font: FONT, font_size: 12, text_color: ACCENT)
            Text (
                "LOCAL DEMO: no system changes",
                font: FONT,
                font_size: 12,
                text_color: SECONDARY
            )
        }
    }
}

#[compose]
fn tool_tile(state: model!(DisplayState), library: model!(Launcher), index: usize) -> Entity {
    let (tool, glyph, title) = TOOLS[index];
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            padding: Padding::all(4),
            row_gap: 3,
            border_width: 2,
            border_color: ${ if library.selected() == index { ACCENT } else { PANEL } },
            bg_color: ${ if library.alternate_theme() { ACCENT_DIM } else { PANEL } }
        ) {
            Text (glyph, font: FONT, font_size: 12, text_color: ACCENT)
            Text (title, font: FONT, font_size: 12, text_color: FOREGROUND)
            Text (
                text: ${
                    match tool {
                        Tool::Sound => if library.sound() { "ON".into() } else { "OFF".into() }
                        Tool::VolumeDown | Tool::VolumeUp => format!("{}%", library.volume()),
                        Tool::BrightnessDown | Tool::BrightnessUp => format!("{}%", library.brightness()),
                        Tool::Theme => {
                            if library.alternate_theme() { "WARM".into() } else { "PAPER".into() }
                        }
                        _ => String::new(),
                    }
                },
                font: FONT,
                font_size: 12,
                text_color: SECONDARY
            )
        } on Tap {
            library.select(index);
            activate_tool(&state, tool);
        }
    }
}

#[compose]
fn wifi(state: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, row_gap: 8) {
            Text (
                text: ${ if state.connected() { "Nearby Wi-Fi: CONNECTED" } else { "Nearby Wi-Fi: OFFLINE" } },
                height: 28,
                font: FONT,
                font_size: 12,
                text_color: FOREGROUND
            )
            Text (
                "Nearby Guest  /  92%",
                height: 28,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                bg_color: PANEL,
                text_color: FOREGROUND
            ) on Tap { state.network(); }
            Text (
                "Nearby Home  /  64%",
                height: 28,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                bg_color: PANEL,
                text_color: FOREGROUND
            ) on Tap { state.network(); }
            Text (text: ${ state.notice() }, font: FONT, font_size: 12, text_color: ACCENT)
            Text (
                "LOCAL DEMO: simulated network",
                font: FONT,
                font_size: 12,
                text_color: SECONDARY
            )
        }
    }
}

#[compose]
fn files(state: model!(Launcher)) -> Entity {
    ui! {
        Column (grow: 1.0, row_gap: 8) {
            Text (
                "/roms  ·  LOCAL DEMO",
                height: 28,
                font: FONT,
                font_size: 12,
                text_color: SECONDARY
            )
            walk PLATFORMS.into_iter().enumerate() with platform {
                Text (
                    platform.1.0,
                    height: 28,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    bg_color: PANEL,
                    text_color: FOREGROUND
                ) on Tap { state.browse_files(platform.0); }
            }
        }
    }
}

#[compose]
fn detail(state: model!(Launcher), screen: Screen) -> Entity {
    let game = state
        .games()
        .iter()
        .find(|game| Some(game.key()) == state.game_key());
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: 8) {
            if let Some (game) = game {
                console (platform(PLATFORMS[game.platform()].1))
                Text (
                    game.name(),
                    height: 28,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: FOREGROUND
                )
                Text (
                    PLATFORMS[game.platform()].0,
                    height: 24,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
            }
            Text (
                if screen == Screen::Manual {
                    "A 确认  B 返回  方向键选择"
                } else {
                    "LOCAL DEMO: no game process is started"
                },
                height: 28,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                text_color: ACCENT
            )
            Text (
                "返回",
                height: 28,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                bg_color: PANEL,
                text_color: FOREGROUND
            ) on Tap { state.back(); }
        }
    }
}
