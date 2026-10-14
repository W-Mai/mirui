use super::{copy, data::*, style::*};
use crate::prelude::*;
use alloc::{string::ToString, vec};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PageKey {
    pub page: Page,
    source: LaunchSource,
    network_only: bool,
    ready: bool,
    connected: bool,
    refresh: u32,
}

#[derive(Clone, Copy)]
pub(super) enum TextField {
    Headline,
    Next,
    HostButton,
    Peer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct StepColors {
    pub background: Color,
    pub foreground: Color,
}

#[model]
pub struct DisplayState {
    view: Snapshot,
    refresh: u32,
    scenario: Scenario,
    fault_delivered: bool,
    leave_to: Page,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            view: Snapshot {
                page: Page::Mode,
                title: "双人冒险".to_string(),
                core: "经典游戏".to_string(),
                room_name: "附近好友的房间".to_string(),
                rooms: vec![
                    RoomRow {
                        name: "小林的房间".into(),
                        detail: "双人冒险 · 1 / 2".into(),
                        signal: 92,
                        available: true,
                    },
                    RoomRow {
                        name: "周末一起玩".into(),
                        detail: "经典游戏 · 1 / 2".into(),
                        signal: 64,
                        available: true,
                    },
                    RoomRow {
                        name: "旅途中的朋友".into(),
                        detail: "".into(),
                        signal: 0,
                        available: false,
                    },
                ],
                entries: vec![
                    GameRow {
                        name: "双人冒险".into(),
                        detail: "一起出发，探索新的地图".into(),
                        directory: false,
                    },
                    GameRow {
                        name: "街头赛车".into(),
                        detail: "两个人的速度比赛".into(),
                        directory: false,
                    },
                    GameRow {
                        name: "方块世界".into(),
                        detail: "单机游戏".into(),
                        directory: false,
                    },
                ],
                ..Snapshot::default()
            },
            refresh: 0,
            scenario: Scenario::Normal,
            fault_delivered: false,
            leave_to: Page::Choice,
        }
    }
}

#[model]
impl DisplayState {
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.view.clone();
        if !self.rooms_visible() {
            snapshot.rooms.clear();
        }
        if self.scenario == Scenario::EmptyGames {
            snapshot.entries.clear();
        }
        snapshot
    }
    #[observe]
    pub fn page_key(&self) -> PageKey {
        PageKey {
            page: self.view.page,
            source: self.view.source,
            network_only: self.view.network_only,
            ready: self.view.ready,
            connected: self.view.connected,
            refresh: self.refresh,
        }
    }
    #[observe]
    pub fn selected(&self) -> usize {
        self.view.selected
    }
    #[observe]
    pub fn presentation(&self) -> Presentation {
        Presentation {
            source: self.view.source,
            outcome: self.view.outcome,
            page: self.view.page,
            phase: self.view.phase,
            network_only: self.view.network_only,
            connected: self.view.connected,
            ready: self.view.ready,
            busy: self.view.busy,
            refreshing: self.view.refreshing,
            rooms_empty: !self.rooms_visible() || self.view.rooms.is_empty(),
            entries_empty: self.scenario == Scenario::EmptyGames || self.view.entries.is_empty(),
        }
    }
    #[observe]
    pub fn stamp(&self) -> Color {
        if self.view.connected {
            ACCENT
        } else {
            ACCENT_DIM
        }
    }
    #[observe]
    pub fn activity(&self) -> [Color; 3] {
        [if copy::loading(&self.presentation()) {
            ACCENT
        } else {
            DIVIDER
        }; 3]
    }
    #[observe]
    pub(super) fn steps(&self) -> [StepColors; 3] {
        let active = match self.view.phase {
            Phase::Connecting => 0,
            Phase::Checking | Phase::Notifying => 1,
            _ => 2,
        };
        core::array::from_fn(|index| StepColors {
            background: if index == active {
                ACCENT
            } else if index < active {
                ACCENT_DIM
            } else {
                PANEL
            },
            foreground: if index == active {
                BACKGROUND
            } else if index < active {
                ACCENT
            } else {
                DISABLED
            },
        })
    }
    pub fn select(&mut self, index: usize) {
        let count = match self.view.page {
            Page::Mode | Page::Choice => 2,
            Page::Games => self.view.entries.len(),
            Page::Rooms => self.view.rooms.len(),
            _ => 1,
        };
        self.view.selected = index.min(count.saturating_sub(1));
    }
    pub fn activate(&mut self, index: usize) {
        self.select(index);
        self.confirm();
    }
    pub fn confirm(&mut self) {
        match self.view.page {
            Page::Mode => {
                self.view.network_only = self.view.selected == 0;
                self.view.page = if self.view.network_only {
                    Page::Choice
                } else {
                    Page::Games
                };
            }
            Page::Games => {
                if self.scenario == Scenario::EmptyGames {
                    return;
                }
                let Some(game) = self.view.entries.get(self.view.selected) else {
                    return;
                };
                self.view.title.clone_from(&game.name);
                self.view.page =
                    if self.view.selected == 2 || self.scenario == Scenario::UnsupportedGame {
                        Page::Unavailable
                    } else {
                        Page::Choice
                    };
            }
            Page::Choice => {
                if self.take_fault(Scenario::ExistingRoom) {
                    self.leave_to = Page::Choice;
                    self.view.page = Page::Leaving;
                    self.set_phase(Phase::RoomActive);
                    return;
                }
                self.view.page = if self.view.selected == 0 {
                    Page::Host
                } else {
                    Page::Rooms
                };
                self.set_phase(if self.view.selected == 0 {
                    Phase::Creating
                } else {
                    Phase::Idle
                });
                if self.view.page == Page::Rooms {
                    self.refresh_rooms();
                }
            }
            Page::Rooms => {
                if !self.rooms_visible() {
                    return;
                }
                let Some(room) = self
                    .view
                    .rooms
                    .get(self.view.selected)
                    .filter(|row| row.available)
                else {
                    return;
                };
                self.view.room_name.clone_from(&room.name);
                self.view.page = Page::Joined;
                self.set_phase(Phase::Connecting);
            }
            Page::Host | Page::Joined => {
                match self.view.phase {
                    Phase::ApprovalPending => self.set_phase(if self.view.network_only {
                        Phase::NetworkConfirming
                    } else {
                        Phase::HostReady
                    }),
                    Phase::HostReady => self.set_phase(Phase::Starting),
                    Phase::NetworkConnected => self.exit(Outcome::KeepNetwork),
                    Phase::Closed | Phase::Failed => self.back_page(),
                    _ => {}
                }
                return;
            }
            Page::Leaving
                if matches!(self.view.phase, Phase::RestoreFailed | Phase::RoomActive) =>
            {
                self.set_phase(Phase::Leaving);
                return;
            }
            Page::Leaving | Page::Unavailable | Page::Outcome => {
                self.back_page();
                return;
            }
        }
        self.view.selected = 0;
    }
    pub fn advance(&mut self) {
        if self.view.page == Page::Rooms && self.view.refreshing {
            self.view.refreshing = false;
            self.refresh = self.refresh.wrapping_add(1);
            return;
        }
        let current = self.view.phase;
        let phase = match current {
            Phase::Creating | Phase::Connecting if self.take_fault(Scenario::ConnectionFailure) => {
                Phase::Failed
            }
            Phase::Creating => Phase::WaitingPeer,
            Phase::WaitingPeer => Phase::PeerPreparing,
            Phase::PeerPreparing => Phase::ApprovalPending,
            Phase::Connecting if self.view.network_only => Phase::ApprovalPendingClient,
            Phase::Connecting => Phase::Checking,
            Phase::Checking => Phase::Notifying,
            Phase::Notifying => Phase::WaitingStart,
            Phase::ApprovalPendingClient => Phase::NetworkConfirming,
            Phase::NetworkConfirming | Phase::NetworkReconnecting => Phase::NetworkConnected,
            Phase::WaitingStart | Phase::HostReady | Phase::NetworkConnected
                if self.take_fault(Scenario::PeerClosed) =>
            {
                Phase::Closed
            }
            Phase::NetworkConnected if self.take_fault(Scenario::Reconnect) => {
                Phase::NetworkReconnecting
            }
            Phase::WaitingStart => Phase::Starting,
            Phase::Starting => {
                self.exit(Outcome::Multiplayer);
                return;
            }
            Phase::Leaving if self.take_fault(Scenario::RestoreFailure) => Phase::RestoreFailed,
            Phase::Leaving => {
                self.view.page = self.leave_to;
                self.set_phase(Phase::Idle);
                self.clear_connection();
                self.view.selected = 0;
                return;
            }
            _ => return,
        };
        self.set_phase(phase);
    }
    pub fn back_page(&mut self) {
        if self.view.page == Page::Leaving {
            if matches!(self.view.phase, Phase::RestoreFailed | Phase::RoomActive) {
                self.set_phase(Phase::Leaving);
            }
            return;
        }
        if matches!(self.view.page, Page::Choice | Page::Unavailable)
            && self.view.source == LaunchSource::Game
        {
            self.exit(Outcome::SinglePlayer);
            return;
        }
        if self.view.page == Page::Mode {
            self.exit(Outcome::System);
            return;
        }
        if matches!(self.view.page, Page::Host | Page::Joined) {
            self.leave_to = if self.view.page == Page::Joined {
                Page::Rooms
            } else {
                Page::Choice
            };
            self.view.page = Page::Leaving;
            self.set_phase(Phase::Leaving);
            return;
        }
        self.view.page = match self.view.page {
            Page::Outcome => {
                self.restart();
                return;
            }
            Page::Games => Page::Mode,
            Page::Choice if self.view.network_only => Page::Mode,
            Page::Choice | Page::Unavailable => Page::Games,
            _ => Page::Choice,
        };
        self.set_phase(Phase::Idle);
        self.view.selected = 0;
        self.clear_connection();
    }
    pub fn restart(&mut self) {
        self.clear_connection();
        self.view.network_only = false;
        self.view.page = if self.view.source == LaunchSource::Game {
            if self.scenario == Scenario::UnsupportedGame {
                Page::Unavailable
            } else {
                Page::Choice
            }
        } else {
            Page::Mode
        };
        self.view.selected = 0;
        self.view.refreshing = false;
        self.fault_delivered = false;
        self.set_phase(Phase::Idle);
    }
    pub fn refresh_rooms(&mut self) {
        if self.view.page != Page::Rooms {
            return;
        }
        self.view.refreshing = true;
        self.view.selected = 0;
        self.refresh = self.refresh.wrapping_add(1);
    }
    pub fn switch_entry(&mut self) {
        self.view.source = match self.view.source {
            LaunchSource::Menu => LaunchSource::Game,
            LaunchSource::Game => LaunchSource::Menu,
        };
        self.view.title = "双人冒险".into();
        self.restart();
    }
    pub fn next_scenario(&mut self) {
        self.scenario = self.scenario.next();
        self.restart();
    }
    #[observe]
    pub fn scenario(&self) -> Scenario {
        self.scenario
    }
    #[observe]
    pub fn rooms_visible(&self) -> bool {
        !self.view.refreshing && self.scenario != Scenario::EmptyRooms
    }
}

impl DisplayState {
    fn take_fault(&mut self, scenario: Scenario) -> bool {
        if self.scenario != scenario || self.fault_delivered {
            return false;
        }
        self.fault_delivered = true;
        true
    }
    fn clear_connection(&mut self) {
        self.view.connected = false;
        self.view.local_address.clear();
        self.view.peers.clear();
    }
    fn set_phase(&mut self, phase: Phase) {
        self.view.phase = phase;
        self.view.ready = matches!(
            phase,
            Phase::HostReady | Phase::WaitingStart | Phase::Starting
        );
        self.view.busy = matches!(
            phase,
            Phase::Creating
                | Phase::Connecting
                | Phase::Checking
                | Phase::Notifying
                | Phase::NetworkConfirming
                | Phase::Starting
                | Phase::Leaving
        );
        self.view.connected = phase == Phase::NetworkConnected;
        if self.view.connected {
            self.view.local_address = "192.168.4.1".into();
            self.view.peers = vec![NetworkPeer {
                name: "好友".into(),
                address: "192.168.4.2".into(),
            }];
        } else if matches!(phase, Phase::Closed | Phase::Failed) {
            self.clear_connection();
        }
    }
    fn exit(&mut self, outcome: Outcome) {
        self.view.outcome = outcome;
        self.view.page = Page::Outcome;
        self.view.busy = false;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub source: LaunchSource,
    pub outcome: Outcome,
    pub page: Page,
    pub phase: Phase,
    pub network_only: bool,
    pub connected: bool,
    pub ready: bool,
    pub busy: bool,
    pub refreshing: bool,
    pub rooms_empty: bool,
    pub entries_empty: bool,
}

impl DisplayStateHandle {
    pub(super) fn status(&self) -> &'static str {
        copy::status_text(&self.presentation())
    }
    pub(super) fn hint(&self) -> &'static str {
        copy::hint_text(&self.presentation())
    }
    pub(super) fn back(&self) -> &'static str {
        let view = self.presentation();
        if view.page == Page::Choice && view.source == LaunchSource::Game {
            "单机游玩"
        } else if matches!(view.phase, Phase::RestoreFailed | Phase::RoomActive) {
            "重试"
        } else if view.page == Page::Leaving {
            "请稍候"
        } else {
            "返回"
        }
    }
    pub(super) fn headline(&self) -> &'static str {
        copy::headline(&self.presentation())
    }
    pub(super) fn next(&self) -> &'static str {
        copy::next_step(&self.presentation())
    }
    pub(super) fn host_button(&self) -> &'static str {
        copy::button_text(&self.presentation())
    }
    pub(super) fn peer(&self) -> &'static str {
        copy::peer_text(&self.presentation())
    }
    pub(super) fn confirm_label(&self) -> &'static str {
        copy::confirm_text(&self.presentation())
    }
    pub(super) fn confirm_visible(&self) -> bool {
        !self.confirm_label().is_empty()
    }
    pub(super) fn text(&self, field: TextField) -> &'static str {
        match field {
            TextField::Headline => self.headline(),
            TextField::Next => self.next(),
            TextField::HostButton => self.host_button(),
            TextField::Peer => self.peer(),
        }
    }
}
