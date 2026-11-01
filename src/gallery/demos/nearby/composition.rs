use super::{
    actor::actor,
    launcher_view::launcher,
    model::{DisplayState, PageKey},
    pages::page,
    style::*,
    widgets::paragraph,
};
use crate::prelude::*;
use crate::ui::widgets::{
    Button, Text,
    page_view::{CreatePage, PageView},
};

#[event]
fn create_page(
    event: event!(CreatePage<PageKey>),
    state: model!(DisplayState),
    metrics: Metrics,
    room: bool,
) -> Entity {
    let mut snapshot = state.snapshot();
    snapshot.page = event.route().page;
    ui! {
        page (&snapshot, state, metrics, room)
    }
}

#[compose]
pub(super) fn build_widgets(state: model!(DisplayState)) {
    ui! {
        Column (grow: 1.0, min_height: 0, bg_color: BACKGROUND) {
            if ${ state.controls() } {
                Row (height: 28, shrink: 0.0, column_gap: 4) {
                    walk [
                        super::launcher::Screen::Favorites,
                        super::launcher::Screen::Platforms,
                        super::launcher::Screen::Search,
                    ]
                        .into_iter() with screen {
                        Button (
                            text: screen.title(),
                            grow: 1.0,
                            min_width: 0,
                            border_radius: 0,
                            font: FONT,
                            font_size: 12
                        ) on Tap { state.launcher().open(screen); }
                    }
                    Button (
                        text: "设置",
                        grow: 1.0,
                        min_width: 0,
                        border_radius: 0,
                        font: FONT,
                        font_size: 12
                    ) on Tap { state.launcher().show_overlay(super::launcher::Overlay::System); }
                }
                Row (
                    height: 30,
                    shrink: 0.0,
                    padding: Padding::all(4),
                    column_gap: 8,
                    align: AlignItems::Center
                ) {
                    Button (
                        text: "Launcher",
                        height: 24,
                        border_radius: 0,
                        font: FONT,
                        font_size: 12
                    ) on Tap { state.launcher().resume(); }
                    Button (
                        id: "nearby_entry",
                        grow: 1.0,
                        min_width: 0,
                        height: 24,
                        border_radius: 0,
                        normal_color: PANEL,
                        text_color: FOREGROUND,
                        font: FONT,
                        font_size: 12
                    ) {
                        Text (
                            text: ${
                                if state.presentation().source == super::data::LaunchSource::Game {
                                    "Entry: in-game"
                                } else {
                                    "Entry: menu"
                                }
                            },
                            font: FONT,
                            font_size: 12,
                            text_color: FOREGROUND
                        )
                    } on Tap { state.switch_entry(); }
                    Button (
                        id: "nearby_scenario",
                        grow: 1.0,
                        min_width: 0,
                        height: 24,
                        border_radius: 0,
                        normal_color: PANEL,
                        text_color: FOREGROUND,
                        font: FONT,
                        font_size: 12
                    ) {
                        Text (
                            text: ${ state.scenario().label() },
                            min_width: 0,
                            font: FONT,
                            font_size: 12,
                            text_color: FOREGROUND,
                            paragraph: paragraph(crate::ui::widgets::TextAlign::Center, 1)
                        )
                    } on Tap { state.next_scenario(); }
                }
                Row (
                    height: 36,
                    shrink: 0.0,
                    padding: Padding::all(4),
                    column_gap: 4,
                    align: AlignItems::Center
                ) {
                    Button (
                        id: "nearby_confirm",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: ACCENT,
                        text_color: ON_ACCENT,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Confirm"),
                    ] on Tap {
                        if state.launcher().active() {
                            super::launcher_view::confirm(&state);
                        } else {
                            state.confirm();
                        }
                    }
                    Button (
                        id: "nearby_back",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: PANEL,
                        text_color: FOREGROUND,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Back"),
                    ] on Tap {
                        if state.launcher().active() {
                            state.launcher().back();
                        } else {
                            state.back_page();
                        }
                    }
                    Button (
                        id: "nearby_advance",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: ACCENT,
                        text_color: ON_ACCENT,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Next"),
                    ] on Tap { state.advance(); }
                    Button (
                        id: "nearby_refresh",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: PANEL,
                        text_color: FOREGROUND,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Refresh"),
                    ] on Tap { state.refresh_rooms(); }
                    Button (
                        id: "nearby_restart",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: PANEL,
                        text_color: FOREGROUND,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Restart"),
                    ] on Tap { state.restart(); }
                }
            }
            if ${ state.launcher().active() || state.return_to_launcher() } {
                launcher (state.clone(), state.launcher())
            } else {
                page_host (state)
                actor (state, false)
            }
        }
    };
}

#[compose]
fn page_host(state: model!(DisplayState)) -> Entity {
    let metrics = state.metrics();
    ui! {
        PageView (
            id: "nearby_pages",
            page: ${ state.page_key() },
            grow: 1.0,
            min_height: 0,
            bg_color: BACKGROUND,
            clip_children: true,
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state, metrics, false))
    }
}

#[compose]
pub(super) fn room_pages(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            id: "nearby_room_pages",
            grow: 1.0,
            min_height: 0,
            page: ${ state.page_key() },
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state, metrics, true))
    }
}
