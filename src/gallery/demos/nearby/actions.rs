use super::configuration_view::{hints as configuration_hints, panel as configuration_panel};
use super::detail_view::details_panel;
use super::launcher::{Launcher, MenuAction, Overlay};
use super::model::DisplayState;
use super::pixel::{Scenery::Margins, backdrop};
use super::style::*;
use super::widgets::{control_key, control_label, shadow, text_block};
use super::wifi_view::{heading as wifi_heading, hints as wifi_hints, panel as wifi_panel};
use crate::prelude::*;
use crate::ui::widgets::{
    Image, Text,
    TextAlign::{Center, Start},
};
use ActionLayout::{Grid, Levels};

#[component]
pub(super) enum ActionAnchor {
    Item { index: usize, layout: ActionLayout },
    Reading,
}

#[derive(Clone, Copy)]
pub(super) enum ActionLayout {
    Grid,
    Levels,
    Details,
    Configuration,
    WifiList,
    WifiOptions,
    WifiKeyboard,
    WifiProgress,
    SearchKeyboard,
}

impl ActionAnchor {
    pub(super) fn index(&self) -> Option<usize> {
        match self {
            Self::Item { index, .. } => Some(*index),
            Self::Reading => None,
        }
    }
    pub(super) fn active(&self, state: &super::launcher::LauncherHandle) -> bool {
        let layout = match self {
            Self::Item { layout, .. } => layout,
            Self::Reading => return state.detail_expanded(),
        };
        match layout {
            ActionLayout::Grid => {
                state.overlay() != Some(Overlay::Controls)
                    && !state.detail_active()
                    && !state.wifi_active()
                    && !state.configuration_active()
            }
            ActionLayout::Levels => state.overlay() == Some(Overlay::Controls),
            ActionLayout::Details => state.detail_active(),
            ActionLayout::Configuration => state.configuration_active(),
            ActionLayout::WifiList => {
                state.wifi_active() && state.wifi_page() == super::wifi::Page::List
            }
            ActionLayout::WifiOptions => {
                state.wifi_active()
                    && !state.wifi_keyboard()
                    && !state.wifi_page().pending()
                    && state.wifi_page() != super::wifi::Page::List
            }
            ActionLayout::WifiKeyboard => state.wifi_keyboard(),
            ActionLayout::WifiProgress => state.wifi_active() && state.wifi_page().pending(),
            ActionLayout::SearchKeyboard => {
                !state.menu()
                    && state.screen() == super::launcher::Screen::Search
                    && state.keyboard()
            }
        }
    }
}

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
        if let Some(index) = anchor.index() {
            state.select_action(index);
        }
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
                        if ${ state.wifi_active() } {
                            wifi_heading (state, metrics)
                        } else {
                            overlay_label (state, None)
                        }
                        View (height: metrics.rule_height(), bg_color: WARNING, shrink: 0.0)
                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap * 2) {
                            if ${ state.configuration_active() } {
                                configuration_panel (state, state.configuration(), metrics)
                            } else {
                                Column (grow: 1.0, min_width: 0, min_height: 0) {
                                    if ${ state.wifi_active() } {
                                        wifi_panel (state, metrics)
                                    } else {
                                        Column (grow: 1.0, min_height: 0, min_width: 0) {
                                            if ${ state.overlay() == Some(Overlay::Controls) } {
                                                level_panel (state, display, metrics)
                                            } else {
                                                Column (grow: 1.0, min_width: 0, min_height: 0) {
                                                    if ${ state.detail_active() } {
                                                        details_panel (state, metrics)
                                                    } else {
                                                        Column (
                                                            grow: 1.0,
                                                            min_width: 0,
                                                            min_height: 0,
                                                            row_gap: metrics.gap
                                                        ) {
                                                            walk 0_usize..3 with line {
                                                                if ${ line * 2 < state.item_count() } {
                                                                    Row (
                                                                        grow: 1.0,
                                                                        min_height: 0,
                                                                        column_gap: metrics.gap
                                                                    ) {
                                                                        walk 0_usize..2 with column {
                                                                            if ${ line * 2 + column < state.item_count() } {
                                                                                action_tile (
                                                                                    state,
                                                                                    display,
                                                                                    line * 2 + column,
                                                                                    metrics,
                                                                                    Grid
                                                                                )
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Row (
                            height: metrics.footer_height(),
                            shrink: 0.0,
                            column_gap: metrics.gap * 2,
                            align: AlignItems::Center
                        ) {
                            if ${ state.configuration_active() } {
                                configuration_hints (state.configuration(), metrics)
                            } else {
                                Row (
                                    grow: 1.0,
                                    min_width: 0,
                                    min_height: 0,
                                    align: AlignItems::Center
                                ) {
                                    if ${ state.wifi_active() } {
                                        wifi_hints (state, metrics)
                                    } else {
                                        Row (
                                            grow: 1.0,
                                            min_width: 0,
                                            column_gap: metrics.gap * 2,
                                            align: AlignItems::Center
                                        ) {
                                            if ${ state.overlay() == Some(Overlay::Controls) && state.menu_selection() < 2 } {
                                                control_key ("左右", metrics, true)
                                                control_label ("调节", metrics)
                                            } else {
                                                control_key ("A", metrics, true)
                                                control_label (
                                                    ${
                                                        if state.detail_expanded() {
                                                            "收起"
                                                        } else if matches!(
                                                            state.overlay(), Some(Overlay::Details(section)) if section !=
                                                            super::details::Section::Overview
                                                        ) {
                                                            "查看来源"
                                                        } else {
                                                            "确认"
                                                        }
                                                    },
                                                    metrics
                                                )
                                            }
                                            if ${ state.detail_expanded() } {
                                                control_key ("上下", metrics, false)
                                                control_label ("滚动", metrics)
                                            }
                                            control_key ("B", metrics, false)
                                            control_label ("返回", metrics)
                                            Column (grow: 1.0, min_width: 0) {
                                                action_reason (state, metrics)
                                            }
                                        }
                                    }
                                }
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
fn level_panel(state: model!(Launcher), display: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            level_row (state, 0, metrics)
            level_row (state, 1, metrics)
            Row (
                height: metrics.action_height() + 4,
                shrink: 0.0,
                column_gap: metrics.gap,
                min_height: 0
            ) {
                action_tile (state, display, 2, metrics, Levels)
                action_tile (state, display, 3, metrics, Levels)
            }
        }
    }
}

fn position_level(
    state: &super::launcher::LauncherHandle,
    index: usize,
    rect: Rect,
    x: Fixed,
) -> bool {
    let Some(&control) = super::launcher::SystemLevel::ALL.get(index) else {
        return false;
    };
    if rect.w <= Fixed::ZERO || !state.item(index).is_some_and(|item| item.enabled) {
        return false;
    }
    let value = (((x - rect.x) / rect.w).clamp(Fixed::ZERO, Fixed::ONE) * 100).to_int() as u8;
    state.select_action(index);
    state.set_level(control, ((value + 2) / 5 * 5).min(100));
    true
}

#[event]
fn level_tap(
    node: entity!(),
    event: event!(Tap),
    rects: query!(crate::ui::ComputedRect),
    state: model!(Launcher),
    index: usize,
) -> bool {
    rects
        .get(node)
        .is_some_and(|rect| position_level(state, index, rect.0, event.x))
}

#[event]
fn level_drag(
    node: entity!(),
    event: event!(DragMove),
    rects: query!(crate::ui::ComputedRect),
    state: model!(Launcher),
    index: usize,
) -> bool {
    rects
        .get(node)
        .is_some_and(|rect| position_level(state, index, rect.0, event.x))
}

#[compose]
fn level_row(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: if metrics.compact { 60 } else { 76 }, min_width: 0) {
            Row (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                column_gap: metrics.gap,
                padding: Padding::all(metrics.gap),
                align: AlignItems::Center,
                bg_color: ${ if state.menu_selection() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == index { ACCENT } else { DIVIDER } },
                transform: ${
                    crate::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if state.menu_selection() == index {
                                super::motion::hop(state.selection_age(), 3.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor::Item {
                    index,
                    layout: ActionLayout::Levels,
                },
            ] {
                View (
                    width: 3,
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${ if state.menu_selection() == index { ACCENT } else { DIVIDER } }
                )
                Column (width: 32, height: 32, shrink: 0.0) {
                    action_icon (state, index)
                }
                Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 4) {
                    Row (
                        height: 24,
                        min_width: 0,
                        shrink: 0.0,
                        align: AlignItems::Center,
                        column_gap: metrics.gap
                    ) {
                        Text (
                            text: ${ state.item(index).map(|item| item.name).unwrap_or_default() },
                            grow: 1.0,
                            min_width: 0,
                            font: font(24),
                            font_size: 24,
                            text_color: ${ action_foreground(&state, index) },
                            paragraph: super::widgets::paragraph(Start, 1)
                        ) [
                            FONT_PAINT,
                        ]
                        View (
                            width: 4,
                            height: 4,
                            bg_color: ${ if state.level(index).is_some_and(|level| level.pending) { WARNING } else { PANEL } }
                        )
                        Text (
                            text: ${
                                state
                                    .level(index)
                                    .and_then(|level| level.value)
                                    .map_or_else(|| "--".into(), |value| alloc::format!("{value}%"))
                            },
                            width: 60,
                            shrink: 0.0,
                            font: font(24),
                            font_size: 24,
                            text_color: ${ action_foreground(&state, index) },
                            paragraph: super::widgets::paragraph(crate::ui::widgets::TextAlign::End, 1)
                        ) [
                            FONT_PAINT,
                        ]
                    }
                    View (
                        height: 14,
                        width: Dimension::percent(100),
                        min_width: 0,
                        shrink: 0.0,
                        container: true,
                        bg_color: BACKGROUND,
                        border_width: 1,
                        border_color: SECONDARY
                    ) {
                        View (
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            height: Dimension::percent(100),
                            width: ${ Dimension::Percent(state.level_positions()[index]) },
                            bg_color: ACCENT
                        )
                        walk 1..20 with segment {
                            View (
                                position: Position::Absolute,
                                left: Dimension::percent(segment * 5),
                                top: 0,
                                height: Dimension::percent(100),
                                width: 2,
                                bg_color: BACKGROUND
                            )
                        }
                        if ${ state.level(index).is_some_and(|level| level.value.is_some()) } {
                            View (
                                position: Position::Absolute,
                                left: ${ Dimension::Percent(state.level_positions()[index]) },
                                top: -4,
                                width: 10,
                                height: 22,
                                bg_color: WARNING,
                                border_width: 2,
                                border_color: FOREGROUND
                            ) [
                                crate::ui::widgets::WidgetTransform(
                                    crate::types::Transform::translate((-5).into(), Fixed::ZERO),
                                ),
                            ]
                        }
                    } on Tap(level_tap::callback(state, index)) on DragMove(level_drag::callback(state, index))
                }
            } on HoverChanged(hover::callback(state)) on Tap { state.select_action(index); }
            shadow (metrics)
        }
    }
}

#[compose]
fn action_tile(
    state: model!(Launcher),
    display: model!(DisplayState),
    index: usize,
    metrics: Metrics,
    layout: ActionLayout,
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
                ActionAnchor::Item {
                    index,
                    layout,
                },
            ] {
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
                        overlay_label (state, Some(index))
                        action_value (state, index, metrics)
                    }
                }
            } on HoverChanged(hover::callback(state)) on Tap {
                state.select_action(index);
                super::launcher_view::confirm(&display);
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn action_value(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            visible: ${ state.item(index).is_some_and(|item| !item.value.is_empty()) },
            shrink: 0.0
        ) {
            text_block (
                ${ state.item(index).map(|item| item.value).unwrap_or_default() },
                ${ action_foreground(&state, index) },
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
            ${
                let reason: alloc::string::String = state
                    .item(state.menu_selection())
                    .map(|item| item.reason.into())
                    .unwrap_or_default();
                if reason.is_empty() { state.notice().into() } else { reason }
            },
            SECONDARY,
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
    Toolbox,
    Info,
    Wifi,
    Network,
    Muted,
    Core,
    Firmware,
    Storage,
}

fn action_glyph(action: MenuAction) -> ActionGlyph {
    use MenuAction::*;
    match action {
        Play => ActionGlyph::Play,
        Multiplayer | ManageNetwork | RestorePlay => ActionGlyph::Signal,
        OpenWifi => ActionGlyph::Wifi,
        NetworkInfo => ActionGlyph::Network,
        ToggleFavorite => ActionGlyph::Favorite,
        ToggleSound | Volume => ActionGlyph::Sound,
        Brightness => ActionGlyph::Brightness,
        ShowControls | ShowGameSettings => ActionGlyph::Settings,
        ShowTools => ActionGlyph::Toolbox,
        SystemInfo | ShowGameDetails | ExpandDetail => ActionGlyph::Info,
        ShowCoreDetails => ActionGlyph::Core,
        ShowFirmwareDetails => ActionGlyph::Firmware,
        ShowSaveDetails => ActionGlyph::Storage,
        FileManager => ActionGlyph::Library,
        ShowPower | Poweroff => ActionGlyph::Power,
        ShowExit | Exit => ActionGlyph::Exit,
        CycleTheme | Back => ActionGlyph::Back,
    }
}

fn action_foreground(state: &super::launcher::LauncherHandle, index: usize) -> ColorToken {
    if !state.item(index).is_some_and(|item| item.enabled) {
        SECONDARY
    } else if state.detail_active() || state.overlay() == Some(Overlay::Controls) && index < 2 {
        FOREGROUND
    } else if state.menu_selection() == index {
        ON_ACCENT
    } else {
        FOREGROUND
    }
}

fn item_glyph(item: Option<super::launcher::MenuItem>) -> ActionGlyph {
    match item {
        Some(item)
            if item.action == MenuAction::Volume
                && item
                    .level
                    .is_some_and(|level| level.muted || level.value == Some(0)) =>
        {
            ActionGlyph::Muted
        }
        Some(item) => action_glyph(item.action),
        None => ActionGlyph::Back,
    }
}

#[compose]
pub(super) fn action_icon(state: model!(Launcher), index: usize) -> Entity {
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
                    walk ${ core::iter::once((item_glyph(state.item(index)), action_foreground(&state, index))) } with icon by icon {
                        Image (
                            src: match icon.0 {
                                ActionGlyph::Core => {
                                    path!(
                                        M 4 4 H 12 V 12 H 4 Z M 5 1 H 7 V 4 H 5 Z M 9 1 H 11 V 4 H 9 Z M 5 12 H 7
                                        V 15 H 5 Z M 9 12 H 11 V 15 H 9 Z M 1 5 H 4 V 7 H 1 Z M 1 9 H 4 V 11 H 1
                                        Z M 12 5 H 15 V 7 H 12 Z M 12 9 H 15 V 11 H 12 Z
                                    )
                                }
                                ActionGlyph::Firmware => {
                                    path!(
                                        M 3 2 H 6 V 4 H 10 V 2 H 13 V 14 H 3 Z M 1 4 H 3 V 6 H 1 Z M 1 8 H 3 V 10
                                        H 1 Z M 1 12 H 3 V 14 H 1 Z M 13 4 H 15 V 6 H 13 Z M 13 8 H 15 V 10 H 13
                                        Z M 13 12 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Storage => {
                                    path!(
                                        M 2 1 H 12 L 15 4 V 15 H 1 V 1 Z M 4 2 V 6 H 11 V 2 Z M 4 9 V 14 H 12 V 9
                                        Z
                                    )
                                }
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
                                ActionGlyph::Toolbox => {
                                    path!(
                                        M 5 1 H 11 V 5 H 15 V 15 H 1 V 5 H 5 Z M 7 3 V 5 H 9 V 3 Z M 2 8 V 10 H 7
                                        V 12 H 9 V 10 H 14 V 8 H 9 V 7 H 7 V 8 Z
                                    )
                                }
                                ActionGlyph::Info => {
                                    path!(
                                        M 3 0 H 13 V 2 H 15 V 14 H 13 V 16 H 3 V 14 H 1 V 2 H 3 Z M 7 3 V 5 H 9 V
                                        3 Z M 7 7 V 13 H 9 V 7 Z
                                    )
                                }
                                ActionGlyph::Wifi => {
                                    path!(
                                        M 1 2 H 15 V 4 H 1 Z M 3 6 H 13 V 8 H 3 Z M 5 10 H 11 V 12 H 5 Z M 7 14 H
                                        9 V 16 H 7 Z
                                    )
                                }
                                ActionGlyph::Network => {
                                    path!(
                                        M 5 0 H 11 V 5 H 9 V 8 H 14 V 11 H 16 V 16 H 10 V 11 H 12 V 10 H 4 V 11 H
                                        6 V 16 H 0 V 11 H 2 V 8 H 7 V 5 H 5 Z
                                    )
                                }
                                ActionGlyph::Muted => {
                                    path!(
                                        M 1 6 H 4 V 4 H 6 V 2 H 8 V 14 H 6 V 12 H 4 V 10 H 1 Z M 10 4 H 12 V 6 H
                                        10 Z M 12 6 H 14 V 10 H 12 Z M 14 4 H 16 V 6 H 14 Z M 10 10 H 12 V 12 H
                                        10 Z M 14 10 H 16 V 12 H 14 Z
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
fn overlay_label(state: model!(Launcher), index: Option<usize>) -> Entity {
    ui! {
        text_block (
            ${
                index
                    .map_or_else(
                        || state.overlay_title(),
                        |index| {
                            state.item(index).map(|item| item.name.into()).unwrap_or_default()
                        },
                    )
            },
            ${ index.map_or(FOREGROUND, |index| action_foreground(&state, index)) },
            24,
            if index.is_none() { Start } else { Center },
            LABEL_LEADING,
            2
        )
    }
}
