use super::{
    launcher_view::launcher,
    model::{DisplayState, PageKey},
    pages,
    style::*,
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
) -> Entity {
    let mut snapshot = state.snapshot();
    snapshot.page = event.route().page;
    ui! { pages::page (&snapshot, state, metrics) }
}

#[compose]
pub(super) fn build_widgets(state: model!(DisplayState)) {
    let metrics = Metrics::default();
    ui! {
        Column (grow: 1.0, bg_color: BACKGROUND) {
            if ${ !state.launcher().active() } {
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
                        text_color: BACKGROUND,
                        font: FONT,
                        font_size: 12
                    ) [
                        Text::label("Confirm"),
                    ] on Tap { state.confirm(); }
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
                    ] on Tap { state.back_page(); }
                    Button (
                        id: "nearby_advance",
                        grow: 1.0,
                        min_width: 0,
                        height: 28,
                        border_radius: 0,
                        normal_color: ACCENT,
                        text_color: BACKGROUND,
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
            if ${ state.launcher().active() } {
                launcher (state.clone(), state.launcher())
            } else {
                page_host (state, metrics)
            }
        }
    };
}

#[compose]
fn page_host(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            id: "nearby_pages",
            page: ${ state.page_key() },
            grow: 1.0,
            min_height: 0,
            bg_color: BACKGROUND,
            clip_children: true,
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state, metrics))
    }
}
