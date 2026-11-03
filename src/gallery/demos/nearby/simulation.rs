use super::data::{Page, Phase, Scenario};
use super::model::DisplayState;
use super::style::*;
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::UserState;
use crate::ui::widgets::{Button, Text};

mod host;
pub(crate) use host::{InspectorHost, Visibility};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Route {
    #[default]
    Host,
    Join,
    Match,
    NetworkHost,
    NetworkJoin,
    InGame,
}

impl Route {
    pub const ALL: [Self; 6] = [
        Self::Host,
        Self::Join,
        Self::Match,
        Self::NetworkHost,
        Self::NetworkJoin,
        Self::InGame,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Host => "Create game",
            Self::Join => "Join game",
            Self::Match => "Find local game",
            Self::NetworkHost => "Create LAN",
            Self::NetworkJoin => "Join LAN",
            Self::InGame => "In-game entry",
        }
    }
    pub fn for_scenario(self, scenario: Scenario) -> Self {
        match scenario {
            Scenario::EmptyRooms => Self::Join,
            Scenario::EmptyGames => Self::Match,
            Scenario::UnsupportedGame => Self::InGame,
            Scenario::Reconnect => Self::NetworkJoin,
            Scenario::ExistingRoom => Self::Host,
            Scenario::RestoreFailure => Self::NetworkHost,
            Scenario::ConnectionFailure | Scenario::PeerClosed
                if matches!(self, Self::Match | Self::InGame) =>
            {
                Self::Host
            }
            _ => self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Advance,
    Failure,
    Timeout,
    Reject,
    PeerLeft,
    Disconnect,
    RestoreFailure,
    RestoreSuccess,
}

impl Action {
    pub const ALL: [Self; 8] = [
        Self::Advance,
        Self::Failure,
        Self::Timeout,
        Self::Reject,
        Self::PeerLeft,
        Self::Disconnect,
        Self::RestoreFailure,
        Self::RestoreSuccess,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Advance => "Deliver reply",
            Self::Failure => "Connection fails",
            Self::Timeout => "Request times out",
            Self::Reject => "Host rejects",
            Self::PeerLeft => "Remote leaves",
            Self::Disconnect => "Connection lost",
            Self::RestoreFailure => "Restore fails",
            Self::RestoreSuccess => "Restore succeeds",
        }
    }
    pub fn allowed(self, page: Page, phase: Phase, network: bool, refreshing: bool) -> bool {
        use Phase::*;
        match self {
            Self::Advance => {
                refreshing
                    || matches!(
                        phase,
                        ResolvingGame
                            | Creating
                            | WaitingPeer
                            | PeerPreparing
                            | Connecting
                            | Checking
                            | Notifying
                            | WaitingStart
                            | Starting
                            | ApprovalPendingClient
                            | NetworkConfirming
                            | NetworkReconnecting
                            | Leaving
                    )
            }
            Self::Failure => matches!(
                phase,
                ResolvingGame | Creating | Connecting | Checking | Notifying | NetworkConfirming
            ),
            Self::Timeout => matches!(
                phase,
                ResolvingGame
                    | WaitingPeer
                    | PeerPreparing
                    | ApprovalPending
                    | Connecting
                    | Checking
                    | Notifying
                    | ApprovalPendingClient
                    | NetworkReconnecting
            ),
            Self::Reject => phase == ApprovalPendingClient,
            Self::PeerLeft => {
                matches!(page, Page::Host | Page::Joined)
                    && matches!(
                        phase,
                        PeerPreparing
                            | ApprovalPending
                            | ApprovalPendingClient
                            | HostReady
                            | WaitingStart
                            | NetworkConnected
                            | NetworkReconnecting
                    )
            }
            Self::Disconnect => network && phase == NetworkConnected,
            Self::RestoreFailure | Self::RestoreSuccess => phase == Leaving,
        }
    }
}

pub(super) const SCENARIOS: [Scenario; 9] = [
    Scenario::Normal,
    Scenario::ConnectionFailure,
    Scenario::RestoreFailure,
    Scenario::ExistingRoom,
    Scenario::EmptyRooms,
    Scenario::Reconnect,
    Scenario::PeerClosed,
    Scenario::EmptyGames,
    Scenario::UnsupportedGame,
];

pub(super) fn step_label(phase: Phase, refreshing: bool) -> &'static str {
    if refreshing {
        return "Finish room scan";
    }
    match phase {
        Phase::ResolvingGame => "Deliver matching games",
        Phase::Creating => "Deliver create reply",
        Phase::WaitingPeer => "Friend joins",
        Phase::PeerPreparing => "Friend is ready",
        Phase::ApprovalPending => "Press A to approve friend",
        Phase::HostReady => "Press A to start game",
        Phase::Connecting => "Deliver connection reply",
        Phase::Checking => "Game check completes",
        Phase::Notifying => "Friend receives readiness",
        Phase::WaitingStart => "Host starts game",
        Phase::Starting => "Game launch completes",
        Phase::ApprovalPendingClient => "Host approves LAN join",
        Phase::NetworkConfirming => "LAN setup completes",
        Phase::NetworkReconnecting => "Connection recovers",
        Phase::Leaving => "Deliver restore reply",
        Phase::RestoreFailed => "Press A to retry restore",
        Phase::NetworkConnected => "Press A to keep network",
        Phase::SelectingGame => "Press A to choose game",
        Phase::RoomActive => "Press A to close old room",
        Phase::Closed | Phase::Failed => "Press A or B to return",
        Phase::Idle => "Choose a room or an action",
    }
}

#[compose]
pub(super) fn controls(state: model!(DisplayState)) -> Entity {
    let advance = Action::Advance;
    ui! {
        Column (
            id: "nearby_simulation",
            width: host::PANEL_WIDTH,
            shrink: 0.0,
            height: Dimension::percent(100),
            min_height: 0,
            padding: Padding::all(8),
            row_gap: 4,
            bg_color: BACKGROUND,
            border_width: 2,
            border_color: ACCENT,
            clip_children: true
        ) {
            Row (height: 28, shrink: 0.0, column_gap: 4, align: AlignItems::Center) {
                Text (
                    "NETWORK SIMULATION",
                    grow: 1.0,
                    font: FONT,
                    font_size: 12,
                    text_color: FOREGROUND
                )
                Button ("Close", width: 52, height: 24, font: FONT, font_size: 12) on Tap { state.toggle_controls(); }
            }
            Text (
                text: ${ alloc::format!("{:?} / {:?}", state.focus_page(), state.presentation().phase) },
                height: 20,
                shrink: 0.0,
                font: FONT,
                font_size: 12,
                text_color: ACCENT
            )
            Text (
                text: ${ state.simulation_step() },
                height: 32,
                shrink: 0.0,
                width: Dimension::percent(100),
                font: FONT,
                font_size: 12,
                text_color: FOREGROUND
            )
            Row (height: 28, shrink: 0.0, column_gap: 4) {
                reply_button (state, advance)
                Button ("Local A", grow: 1.0, min_width: 0, font: FONT, font_size: 12) on Tap {
                    if state.launcher().active() {
                        super::launcher_view::confirm(&state);
                    } else {
                        state.confirm();
                    }
                }
                Button ("Local B", grow: 1.0, min_width: 0, font: FONT, font_size: 12) on Tap {
                    if state.launcher().active() {
                        state.launcher().back();
                    } else {
                        state.back_page();
                    }
                }
            }
            Row (height: 28, shrink: 0.0, column_gap: 4) {
                Button ("Repeat case", grow: 1.0, min_width: 0, font: FONT, font_size: 12) on Tap { state.run_simulation(state.simulation_route(), state.scenario()); }
                Button ("Stop case", grow: 1.0, min_width: 0, font: FONT, font_size: 12) on Tap { state.stop_simulation(); }
            }
            Column (
                id: "nearby_simulation_options",
                grow: 1.0,
                min_height: 0,
                row_gap: 4,
                clip_children: true
            ) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                Text (
                    "FLOW - starts a normal session",
                    height: 20,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
                walk 0_usize..3 with row {
                    Row (height: 28, shrink: 0.0, column_gap: 4) {
                        walk [Route::ALL[row * 2], Route::ALL[row * 2 + 1]].into_iter() with route {
                            Button (
                                route.label(),
                                grow: 1.0,
                                min_width: 0,
                                font: FONT,
                                font_size: 12,
                                normal_color: ${ if state.simulation_route() == route { ACCENT } else { PANEL } },
                                text_color: ${ if state.simulation_route() == route { ON_ACCENT } else { FOREGROUND } }
                            ) on Tap { state.run_simulation(route, Scenario::Normal); }
                        }
                    }
                }
                Text (
                    "CASE - starts a fresh attempt",
                    height: 20,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
                walk 0_usize..3 with row {
                    Row (height: 28, shrink: 0.0, column_gap: 4) {
                        walk [SCENARIOS[row * 3], SCENARIOS[row * 3 + 1], SCENARIOS[row * 3 + 2]].into_iter() with scenario {
                            Button (
                                scenario.label(),
                                grow: 1.0,
                                min_width: 0,
                                font: FONT,
                                font_size: 12,
                                normal_color: ${ if state.scenario() == scenario { ACCENT } else { PANEL } },
                                text_color: ${ if state.scenario() == scenario { ON_ACCENT } else { FOREGROUND } }
                            ) on Tap { state.run_simulation(state.simulation_route().for_scenario(scenario), scenario); }
                        }
                    }
                }
                Text (
                    "REMOTE REPLIES / FAULTS",
                    height: 20,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
                walk Action::ALL.into_iter().skip(1) with action {
                    Row (height: 28, shrink: 0.0) {
                        reply_button (state, action)
                    }
                }
                Text (
                    "ORDINARY WI-FI / LOCAL MOCK",
                    height: 20,
                    shrink: 0.0,
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
                walk super::wifi::Scenario::ALL.into_iter() with scenario {
                    Button (
                        scenario.label(),
                        height: 28,
                        shrink: 0.0,
                        min_width: 0,
                        font: FONT,
                        font_size: 12,
                        normal_color: ${
                            if state.launcher().wifi_active() && state.launcher().wifi_scenario() == scenario {
                                ACCENT
                            } else {
                                PANEL
                            }
                        },
                        text_color: ${
                            if state.launcher().wifi_active() && state.launcher().wifi_scenario() == scenario {
                                ON_ACCENT
                            } else {
                                FOREGROUND
                            }
                        }
                    ) on Tap {
                        state.launcher().resume();
                        state.launcher().set_wifi_scenario(scenario);
                    }
                }
                Text (
                    "Grey replies do not apply to this state. Local A/B keep the app confirmation and cancel paths.",
                    height: 60,
                    shrink: 0.0,
                    width: Dimension::percent(100),
                    font: FONT,
                    font_size: 12,
                    text_color: SECONDARY
                )
            }
        } on Tap { return true; }
    }
}

#[compose]
fn reply_button(state: model!(DisplayState), action: Action) -> Entity {
    ui! {
        Row (grow: 1.0, min_width: 0, height: Dimension::percent(100)) {
            if ${ state.simulation_allowed(action) } {
                Button (
                    action.label(),
                    grow: 1.0,
                    min_width: 0,
                    normal_color: PANEL,
                    text_color: FOREGROUND,
                    font: FONT,
                    font_size: 12
                ) on Tap { state.simulate(action); }
            } else {
                Button (
                    action.label(),
                    grow: 1.0,
                    min_width: 0,
                    normal_color: PANEL,
                    text_color: DISABLED,
                    font: FONT,
                    font_size: 12
                ) [
                    UserState::Disabled,
                    crate::ui::IgnoreHitTest,
                ]
            }
        }
    }
}
