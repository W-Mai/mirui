use super::consoles::{Platform, console};
use super::launcher::{Launcher, MenuAction, Overlay, PLATFORMS};
use super::model::DisplayState;
use super::pixel::{Scenery::Margins, backdrop};
use super::style::*;
use super::widgets::{control_key, control_label, shadow, text_block};
use crate::prelude::*;
use crate::ui::widgets::{
    Image,
    TextAlign::{Center, Start},
};

#[component]
pub(super) struct ActionAnchor(pub usize);

#[event]
fn hover(
    node: entity!(),
    event: event!(crate::input::event::hover::HoverChanged),
    anchors: query!(ActionAnchor),
    state: model!(Launcher),
) {
    if event.hovered
        && let Some(anchor) = anchors.get(node)
    {
        state.select_action(anchor.0);
    }
}

#[compose]
pub(super) fn overlay(display: model!(DisplayState), state: model!(Launcher)) -> Entity {
    let metrics = display.metrics();
    ui! {
        graphical_overlay (display, state, metrics)
    }
}

#[compose]
fn graphical_overlay(
    display: model!(DisplayState),
    state: model!(Launcher),
    metrics: Metrics,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            crate::ui::IgnoreHitTest,
        ] {
            if ${ state.active() && state.menu() } {
                Column (
                    id: "nearby_actions_scrim",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: metrics.modal_padding(),
                    justify: JustifyContent::Center,
                    bg_color: Color::rgba(7, 16, 12, 185)
                ) {
                    Column (
                        id: "nearby_actions_panel",
                        grow: 1.0,
                        min_height: 0,
                        padding: Padding::all(metrics.gap * 2),
                        row_gap: metrics.gap,
                        bg_color: BACKGROUND,
                        border_width: 2,
                        border_color: DIVIDER
                    ) {
                        backdrop (Margins)
                        overlay_label (state, None, metrics)
                        View (height: metrics.rule_height(), bg_color: WARNING, shrink: 0.0)
                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap * 2) {
                            if ${ state.overlay() == Some(Overlay::Game) } {
                                Column (width: Dimension::percent(38), shrink: 0.0, min_height: 0) {
                                    preview (state)
                                    shadow (metrics)
                                }
                            }
                            Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
                                walk 0_usize..3 with line {
                                    if ${ line * 2 < state.item_count() } {
                                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) {
                                            walk 0_usize..2 with column {
                                                if ${ line * 2 + column < state.item_count() } {
                                                    action_tile (
                                                        state,
                                                        display.clone(),
                                                        line * 2 + column,
                                                        metrics
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Row (
                            height: metrics.footer_height(),
                            column_gap: metrics.gap * 2,
                            align: AlignItems::Center
                        ) {
                            control_key ("A", metrics, true)
                            control_label (|| "确认".into(), metrics)
                            control_key ("B", metrics, false)
                            control_label (|| "返回".into(), metrics)
                            Column (grow: 1.0, min_width: 0) {
                                action_reason (state, metrics)
                            }
                        }
                    }
                    shadow (metrics)
                }
            }
        }
    }
}

#[compose]
fn action_tile(
    state: model!(Launcher),
    display: model!(DisplayState),
    index: usize,
    metrics: Metrics,
) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${
                crate::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.menu_selection() == index {
                            super::motion::hop(state.selection_age(), 4.0)
                        } else {
                            0
                        },
                    ),
                )
            }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                padding: Padding::all(metrics.gap),
                row_gap: metrics.gap / 2,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                bg_color: ${
                    if state.menu_selection() == index
                        && state.item(index).is_some_and(|item| item.enabled)
                    {
                        ACCENT
                    } else {
                        PANEL
                    }
                },
                border_width: 2,
                border_color: ${ if state.menu_selection() == index { WARNING } else { DIVIDER } }
            ) [
                ActionAnchor(index),
            ] {
                if ${ metrics.compact || state.item(index).is_some_and(|item| { !item.value.is_empty() }) } {
                    Row (
                        grow: 1.0,
                        min_height: 0,
                        width: Dimension::percent(100),
                        column_gap: metrics.gap,
                        align: AlignItems::Center
                    ) {
                        Column (width: 32, height: 32, shrink: 0.0) {
                            action_icon (state, index)
                        }
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            row_gap: metrics.gap / 2,
                            justify: JustifyContent::Center
                        ) {
                            overlay_label (state, Some(index), metrics)
                            action_value (state, index, metrics)
                        }
                    }
                } else {
                    action_icon (state, index)
                    overlay_label (state, Some(index), metrics)
                    action_value (state, index, metrics)
                }
            } on HoverChanged(hover::callback(state.clone())) on Tap {
                state.select_action(index);
                super::launcher_view::confirm(&display);
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn action_value(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    let foreground = state.clone();
    ui! {
        Column (
            visible: ${ state.item(index).is_some_and(|item| !item.value.is_empty()) },
            shrink: 0.0
        ) {
            text_block (
                move || state.item(index).map(|item| item.value).unwrap_or_default(),
                move || action_foreground(&foreground, index),
                metrics.small,
                Center,
                LABEL_LEADING,
                1
            )
        }
    }
}

#[compose]
fn action_reason(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        text_block (
            move || {
                let reason: alloc::string::String = state
                    .item(state.menu_selection())
                    .map(|item| item.reason.into())
                    .unwrap_or_default();
                if reason.is_empty() { state.notice().into() } else { reason }
            },
            || SECONDARY,
            metrics.small,
            Start,
            LABEL_LEADING,
            1
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionGlyph {
    Play,
    Library,
    Signal,
    Favorite,
    Sound,
    Brightness,
    Settings,
    Power,
    Exit,
    Back,
}

fn action_glyph(action: MenuAction) -> ActionGlyph {
    use MenuAction::*;
    match action {
        Play => ActionGlyph::Play,
        Multiplayer | ManageNetwork | RestorePlay | OpenWifi | NetworkInfo => ActionGlyph::Signal,
        ToggleFavorite => ActionGlyph::Favorite,
        ToggleSound | VolumeDown | VolumeUp => ActionGlyph::Sound,
        BrightnessDown | BrightnessUp => ActionGlyph::Brightness,
        ShowControls | ShowTools | SystemInfo => ActionGlyph::Settings,
        FileManager => ActionGlyph::Library,
        ShowPower | Poweroff => ActionGlyph::Power,
        ShowExit | Exit => ActionGlyph::Exit,
        CycleTheme | Back => ActionGlyph::Back,
    }
}

fn action_foreground(state: &super::launcher::LauncherHandle, index: usize) -> ColorToken {
    if !state.item(index).is_some_and(|item| item.enabled) {
        SECONDARY
    } else if state.menu_selection() == index {
        ON_ACCENT
    } else {
        FOREGROUND
    }
}

#[compose]
fn action_icon(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 16,
            max_height: 32,
            width: Dimension::percent(100),
            align: AlignItems::Center
        ) {
            if ${ state.item(index).is_some_and(|item| item.action == MenuAction::CycleTheme) } {
                Row (
                    grow: 1.0,
                    width: Dimension::percent(70),
                    column_gap: 3,
                    padding: Padding::all(2),
                    bg_color: PANEL,
                    border_width: 1,
                    border_color: ${ action_foreground(&state, index) }
                ) {
                    walk [ACCENT, WARNING, CLAY, SKY].into_iter() with color {
                        View (grow: 1.0, height: Dimension::percent(100), bg_color: color)
                    }
                }
            } else {
                Column (width: 32, grow: 1.0, min_height: 0) {
                    walk ${
                        core::iter::once((
                            action_glyph(state.item(index).map_or(MenuAction::Back, |item| item.action)),
                            action_foreground(&state, index),
                        ))
                    } with icon by icon {
                        Image (
                            src: match icon.0 {
                                ActionGlyph::Play => {
                                    path!(M 3 2 H 6 V 4 H 10 V 7 H 14 V 10 H 10 V 13 H 6 V 15 H 3 Z)
                                }
                                ActionGlyph::Library => {
                                    path!(M 1 3 H 7 V 5 H 15 V 14 H 1 Z M 3 7 V 12 H 13 V 7 Z)
                                }
                                ActionGlyph::Signal => {
                                    path!(
                                        M 7 1 H 9 V 10 H 7 Z M 4 4 H 6 V 12 H 4 Z M 10 4 H 12 V 12 H 10 Z M 1 7 H
                                        3 V 14 H 1 Z M 13 7 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Favorite => {
                                    path!(
                                        M 6 0 H 10 V 4 H 16 V 7 H 12 V 11 H 14 V 16 H 10 V 13 H 6 V 16 H 2 V 11 H
                                        4 V 7 H 0 V 4 H 6 Z
                                    )
                                }
                                ActionGlyph::Sound => {
                                    path!(
                                        M 1 6 H 4 V 4 H 6 V 2 H 8 V 14 H 6 V 12 H 4 V 10 H 1 Z M 10 5 H 12 V 11 H
                                        10 Z M 13 2 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Power => {
                                    path!(
                                        M 7 0 H 9 V 8 H 7 Z M 3 3 H 5 V 5 H 3 V 11 H 5 V 13 H 11 V 11 H 13 V 5 H
                                        11 V 3 H 13 V 5 H 15 V 11 H 13 V 13 H 11 V 15 H 5 V 13 H 3 V 11 H 1 V 5 H
                                        3 Z
                                    )
                                }
                                ActionGlyph::Exit => {
                                    path!(
                                        M 1 1 H 8 V 3 H 3 V 13 H 8 V 15 H 1 Z M 11 3 H 13 V 5 H 15 V 7 H 16 V 9 H
                                        15 V 11 H 13 V 13 H 11 V 10 H 5 V 6 H 11 Z
                                    )
                                }
                                ActionGlyph::Back => {
                                    path!(
                                        M 6 2 H 8 V 6 H 15 V 10 H 8 V 14 H 6 V 12 H 4 V 10 H 2 V 6 H 4 V 4 H 6 Z
                                    )
                                }
                                ActionGlyph::Brightness => {
                                    path!(
                                        M 6 4 H 10 V 6 H 12 V 10 H 10 V 12 H 6 V 10 H 4 V 6 H 6 Z M 7 0 H 9 V 2 H
                                        7 Z M 7 14 H 9 V 16 H 7 Z M 0 7 H 2 V 9 H 0 Z M 14 7 H 16 V 9 H 14 Z M 2
                                        2 H 4 V 4 H 2 Z M 12 2 H 14 V 4 H 12 Z M 2 12 H 4 V 14 H 2 Z M 12 12 H 14
                                        V 14 H 12 Z
                                    )
                                }
                                ActionGlyph::Settings => {
                                    path!(
                                        M 5 1 H 11 V 3 H 13 V 5 H 15 V 11 H 13 V 13 H 11 V 15 H 5 V 13 H 3 V 11 H
                                        1 V 5 H 3 V 3 H 5 Z M 6 5 V 6 H 5 V 10 H 6 V 11 H 10 V 10 H 11 V 6 H 10 V
                                        5 Z
                                    )
                                }
                            },
                            viewbox: 16,
                            width: 32,
                            height: Dimension::percent(100),
                            color: icon.1
                        )
                    }
                }
            }
        }
    }
}

#[compose]
fn overlay_label(state: model!(Launcher), index: Option<usize>, metrics: Metrics) -> Entity {
    let text = state.clone();
    ui! {
        text_block (
            move || {
                index
                    .map_or_else(
                        || text.overlay_title(),
                        |index| {
                            text.item(index).map(|item| item.name.into()).unwrap_or_default()
                        },
                    )
            },
            move || { index.map_or(FOREGROUND, |index| action_foreground(&state, index)) },
            if index.is_none() { metrics.title } else { metrics.body },
            if index.is_none() { Start } else { Center },
            LABEL_LEADING,
            2
        )
    }
}

#[compose]
fn preview(state: model!(Launcher)) -> Entity {
    let platform = state
        .games()
        .iter()
        .find(|game| Some(game.key()) == state.game_key())
        .map_or(Platform::Unknown, |game| {
            Platform::from_id(PLATFORMS[game.platform()].1)
        });
    ui! {
        Column (
            id: "nearby_actions_preview",
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(2),
            justify: JustifyContent::Center,
            row_gap: 8
        ) {
            console (platform)
        }
    }
}
