use super::{copy, data::*, style::*};
use crate::prelude::*;
use crate::ui::widgets::navigation::{PageDirection, PageTransition};
use alloc::{string::ToString, vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageKey {
    pub page: Page,
    source: LaunchSource,
    network_only: bool,
    session: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TextField {
    Headline,
    Next,
    HostButton,
    Peer,
    RoomName,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct StepColors {
    pub background: ColorToken,
    pub foreground: ColorToken,
}

#[model]
pub struct DisplayState {
    #[observe]
    pub return_to_launcher: bool,
    #[observe]
    pub controls: bool,
    #[observe]
    pub(super) view_revision: u64,
    #[observe]
    pub(super) actor_stage: bool,
    #[observe]
    pub(super) motion: super::motion::Frame,
    timeline: super::motion::Timeline,
    elapsed_ms: u64,
    feedback: super::feedback::Feedback,
    #[observe]
    pub(super) stamp: Color,
    #[observe]
    pub(super) activity: [Color; 3],
    #[observe]
    pub(super) metrics: Metrics,
    #[observe]
    pub(super) deck_height: Fixed,
    pub launcher: model!(owned super::launcher::Launcher),
    view: Snapshot,
    session: u32,
    scenario: Scenario,
    fault_delivered: bool,
    leave_to: Page,
    direction: PageDirection,
    room_selection: usize,
    mode_selection: usize,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            return_to_launcher: false,
            controls: false,
            view_revision: 0,
            actor_stage: true,
            motion: super::motion::Frame::default(),
            timeline: super::motion::Timeline::default(),
            elapsed_ms: 0,
            feedback: super::feedback::Feedback::default(),
            stamp: super::palette::theme(0).resolve(ACCENT_DIM),
            activity: [super::palette::theme(0).resolve(BACKGROUND); 3],
            metrics: Metrics::default(),
            deck_height: Fixed::ZERO,
            launcher: super::launcher::Launcher::default().into(),
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
            session: 0,
            scenario: Scenario::Normal,
            fault_delivered: false,
            leave_to: Page::Choice,
            direction: PageDirection::Forward,
            room_selection: 0,
            mode_selection: 0,
        }
    }
}

#[model]
impl DisplayState {
    pub fn toggle_controls(&mut self) {
        self.controls = !self.controls;
    }
    fn heading_value(&self) -> alloc::string::String {
        match self.view.page {
            Page::Lobby => "附近正在玩的伙伴".into(),
            Page::Candidates => "选择本机的对应游戏".into(),
            Page::Mode => "选择连接方式".into(),
            Page::Choice if self.view.network_only => "和好友建立局域网".into(),
            Page::Choice => "和好友一起，开始游戏".into(),
            Page::Host => {
                if self.view.room_name.is_empty() {
                    "正在创建房间".into()
                } else {
                    self.view.room_name.clone()
                }
            }
            Page::Rooms => "选择好友的房间".into(),
            Page::Games => self.view.title.clone(),
            Page::Joined => "加入房间".into(),
            Page::Leaving => "退出联机".into(),
            Page::Unavailable => "附近联机".into(),
            Page::Outcome => "LOCAL DEMO".into(),
        }
    }
    fn devices_value(&self) -> alloc::vec::Vec<NetworkPeer> {
        let mut devices = alloc::vec![NetworkPeer {
            name: "本机".into(),
            address: if self.view.local_address.is_empty() {
                "等待分配地址".into()
            } else {
                self.view.local_address.clone()
            }
        }];
        devices.extend(self.view.peers.iter().cloned());
        devices
    }
    fn room_name_value(&self) -> alloc::string::String {
        self.view.room_name.clone()
    }
    fn subtitle_value(&self) -> alloc::string::String {
        if matches!(self.view.page, Page::Mode | Page::Lobby) {
            "连接附近的掌机".into()
        } else if self.view.network_only {
            "仅组网 · 局域网连接".into()
        } else if matches!(self.view.page, Page::Games | Page::Candidates) {
            self.view.core.clone()
        } else {
            alloc::format!("当前游戏  {}", self.view.title)
        }
    }
    fn library_counter_value(&self) -> Option<(usize, usize)> {
        (!self.view.entries.is_empty()).then_some((self.view.selected + 1, self.view.entries.len()))
    }
    fn selected_title_value(&self) -> alloc::string::String {
        self.view
            .entries
            .get(self.view.selected)
            .map(|row| row.name.clone())
            .unwrap_or_default()
    }
    pub(super) fn set_actor_stage(&mut self, stage: bool) {
        if self.actor_stage != stage {
            self.actor_stage = stage;
        }
    }
    #[observe]
    pub(super) fn actor_motion(&self) -> super::motion::Frame {
        if matches!(self.view.page, Page::Host | Page::Joined | Page::Leaving) {
            super::motion::Frame {
                page: self.view.page,
                ..self.motion.session()
            }
        } else {
            self.motion.walker()
        }
    }
    #[observe]
    pub(super) fn actor_position(&self) -> Fixed {
        if matches!(self.view.page, Page::Host | Page::Joined | Page::Leaving) {
            self.motion.host
        } else {
            self.motion.walker
        }
    }
    pub(super) fn animate(&mut self, delta: u16, theme: &crate::ui::Theme) {
        self.elapsed_ms = self.elapsed_ms.saturating_add(u64::from(delta));
        let selected = match self.view.page {
            Page::Choice
            | Page::Mode
            | Page::Rooms
            | Page::Lobby
            | Page::Games
            | Page::Candidates => Some(self.view.selected),
            Page::Host | Page::Joined if !copy::confirm_text(&self.presentation()).is_empty() => {
                Some(0)
            }
            _ => None,
        };
        let frame = self.timeline.sample(
            &self.view,
            selected,
            self.elapsed_ms,
            self.feedback.press_sequence,
        );
        if self.motion != frame {
            self.motion = frame;
        }
        let loading = copy::loading(&self.presentation());
        let (stamp, activity) = self.feedback.sample(self.elapsed_ms, loading, theme);
        if self.stamp != stamp {
            self.stamp = stamp;
        }
        if self.activity != activity {
            self.activity = activity;
        }
    }
    #[observe]
    pub(super) fn current_game(&self) -> bool {
        self.view.source == LaunchSource::Game
    }
    #[observe]
    pub(super) fn choice_motion(&self) -> super::motion::Frame {
        self.motion.choice()
    }
    #[observe]
    pub(super) fn walker_motion(&self) -> super::motion::Frame {
        self.motion.walker()
    }
    #[observe]
    pub(super) fn session_motion(&self) -> super::motion::Frame {
        self.motion.session()
    }
    #[observe]
    pub(super) fn signal_motion(&self) -> super::motion::Frame {
        self.motion.signal()
    }
    #[observe]
    pub(super) fn search_motion(&self) -> super::motion::Frame {
        self.motion.search()
    }
    #[observe]
    pub(super) fn card_motion(&self) -> super::motion::Frame {
        self.motion.cards()
    }
    pub(super) fn resize(&mut self, height: u16) {
        let metrics = Metrics::for_height(height);
        if self.metrics != metrics {
            self.metrics = metrics;
        }
    }
    pub fn open_network(&mut self) {
        self.return_to_launcher = true;
        self.launcher.close();
        self.view.source = LaunchSource::Menu;
        self.view.lobby = true;
        self.restart();
    }
    pub fn launch_game(&mut self, name: alloc::string::String, multiplayer: bool) {
        self.return_to_launcher = true;
        self.launcher.close();
        self.view.source = LaunchSource::Game;
        self.view.lobby = false;
        self.view.title = name;
        self.restart();
        if !multiplayer {
            self.navigate(Page::Unavailable, PageDirection::Forward);
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.view.clone();
        if self.scenario == Scenario::EmptyGames {
            snapshot.entries.clear();
        }
        snapshot
    }
    #[observe]
    pub fn page_key(&self) -> PageKey {
        PageKey {
            page: self.view.scene_page(),
            source: self.view.source,
            network_only: !matches!(self.view.page, Page::Mode | Page::Games | Page::Unavailable)
                && self.view.network_only,
            session: self.session,
        }
    }
    #[observe]
    pub fn transition(&self) -> PageTransition {
        PageTransition::slide(crate::anim::SNAPPY).with_direction(self.direction)
    }
    #[observe]
    pub fn selected(&self) -> usize {
        self.view.selected
    }
    #[observe]
    pub(super) fn focus_page(&self) -> Page {
        self.view.page
    }
    pub(super) fn set_deck_height(&mut self, height: Fixed) {
        if self.deck_height != height {
            self.deck_height = height;
        }
    }
    pub fn expand_discovery(&mut self, expanded: bool) {
        if self.view.lobby && matches!(self.view.page, Page::Lobby | Page::Mode) {
            if expanded == (self.view.page == Page::Mode) {
                return;
            }
            if expanded {
                self.room_selection = self.view.selected;
            } else {
                self.mode_selection = self.view.selected;
            }
            self.navigate(
                if expanded { Page::Mode } else { Page::Lobby },
                PageDirection::Forward,
            );
            self.view.selected = if expanded {
                self.mode_selection
            } else {
                self.room_selection
            };
        }
    }
    pub(super) fn choose_connection(&mut self, index: usize) {
        if self.view.lobby && self.view.scene_page() == Page::Lobby {
            self.expand_discovery(true);
        }
        self.activate(index);
    }
    pub(super) fn choose_room(&mut self, index: usize) {
        if self.view.lobby && self.view.scene_page() == Page::Lobby {
            self.expand_discovery(false);
        }
        self.activate(index);
    }
    #[observe]
    pub fn presentation(&self) -> Presentation {
        Presentation {
            lobby: self.view.lobby,
            return_to_launcher: self.return_to_launcher,
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
                ON_ACCENT
            } else if index < active {
                ACCENT
            } else {
                DISABLED
            },
        })
    }
    pub fn select(&mut self, index: usize) {
        let count = match self.view.page {
            Page::Mode if self.current_game() => 3,
            Page::Mode | Page::Choice => 2,
            Page::Games | Page::Candidates => self.view.entries.len(),
            Page::Rooms | Page::Lobby => self.view.rooms.len(),
            _ => 1,
        };
        let selected = index.min(count.saturating_sub(1));
        if self.view.selected != selected {
            self.view.selected = selected;
            self.view_revision = self.view_revision.wrapping_add(1);
        }
    }
    pub fn activate(&mut self, index: usize) {
        self.select(index);
        self.confirm();
    }
    pub fn confirm(&mut self) {
        self.feedback.press(self.elapsed_ms);
        match self.view.page {
            Page::Mode => {
                if self.view.selected == 2 && self.current_game() {
                    self.view.network_only = false;
                    self.navigate(Page::Host, PageDirection::Forward);
                    self.set_phase(Phase::Creating);
                    return;
                }
                self.view.network_only = self.view.selected == 0;
                let page = if self.view.network_only {
                    Page::Choice
                } else {
                    Page::Games
                };
                self.navigate(page, PageDirection::Forward);
            }
            Page::Games | Page::Candidates => {
                if self.scenario == Scenario::EmptyGames {
                    return;
                }
                let Some(game) = self.view.entries.get(self.view.selected) else {
                    return;
                };
                self.view.title.clone_from(&game.name);
                if self.view.page == Page::Candidates {
                    self.navigate(Page::Joined, PageDirection::Forward);
                    self.set_phase(Phase::Connecting);
                    self.view.selected = 0;
                    return;
                }
                let page = if self.view.selected == 2 || self.scenario == Scenario::UnsupportedGame
                {
                    Page::Unavailable
                } else {
                    Page::Choice
                };
                self.navigate(page, PageDirection::Forward);
            }
            Page::Choice => {
                if self.take_fault(Scenario::ExistingRoom) {
                    self.leave_to = Page::Choice;
                    self.navigate(Page::Leaving, PageDirection::Forward);
                    self.set_phase(Phase::RoomActive);
                    return;
                }
                let page = if self.view.selected == 0 {
                    Page::Host
                } else {
                    Page::Rooms
                };
                self.navigate(page, PageDirection::Forward);
                self.set_phase(if self.view.selected == 0 {
                    Phase::Creating
                } else {
                    Phase::Idle
                });
                if self.view.page == Page::Rooms {
                    self.refresh_rooms();
                }
            }
            Page::Rooms | Page::Lobby => {
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
                let resolve = self.view.page == Page::Lobby;
                self.navigate(Page::Joined, PageDirection::Forward);
                self.set_phase(if resolve {
                    Phase::ResolvingGame
                } else {
                    Phase::Connecting
                });
            }
            Page::Host | Page::Joined => {
                match self.view.phase {
                    Phase::ApprovalPending => self.set_phase(if self.view.network_only {
                        Phase::NetworkConfirming
                    } else {
                        Phase::HostReady
                    }),
                    Phase::HostReady => self.set_phase(Phase::Starting),
                    Phase::NetworkConnected => {
                        self.exit(Outcome::KeepNetwork, PageDirection::Forward)
                    }
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
        if matches!(self.view.page, Page::Rooms | Page::Lobby | Page::Mode) && self.view.refreshing
        {
            self.view.refreshing = false;
            self.view_revision = self.view_revision.wrapping_add(1);
            return;
        }
        let current = self.view.phase;
        let phase = match current {
            Phase::ResolvingGame => {
                self.navigate(Page::Candidates, PageDirection::Forward);
                self.view.selected = 0;
                Phase::SelectingGame
            }
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
                self.exit(Outcome::Multiplayer, PageDirection::Forward);
                return;
            }
            Phase::Leaving if self.take_fault(Scenario::RestoreFailure) => Phase::RestoreFailed,
            Phase::Leaving => {
                self.navigate(self.leave_to, PageDirection::Backward);
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
            self.exit(Outcome::SinglePlayer, PageDirection::Backward);
            return;
        }
        if self.view.page == Page::Mode && self.view.lobby {
            self.expand_discovery(false);
            return;
        }
        if matches!(self.view.page, Page::Mode | Page::Lobby) {
            self.exit(Outcome::System, PageDirection::Backward);
            return;
        }
        if matches!(self.view.page, Page::Host | Page::Joined) {
            self.leave_to = if self.view.page == Page::Joined {
                if self.view.lobby {
                    Page::Lobby
                } else {
                    Page::Rooms
                }
            } else {
                Page::Choice
            };
            self.navigate(Page::Leaving, PageDirection::Backward);
            self.set_phase(Phase::Leaving);
            return;
        }
        let page = match self.view.page {
            Page::Outcome => {
                self.restart();
                return;
            }
            Page::Games => Page::Mode,
            Page::Choice if self.view.network_only => Page::Mode,
            Page::Candidates => Page::Lobby,
            Page::Choice | Page::Unavailable => Page::Games,
            _ => Page::Choice,
        };
        self.navigate(page, PageDirection::Backward);
        self.set_phase(Phase::Idle);
        self.view.selected = 0;
        self.clear_connection();
    }
    pub fn restart(&mut self) {
        self.session = self.session.wrapping_add(1);
        self.clear_connection();
        self.view.network_only = false;
        let page = if self.view.source == LaunchSource::Game {
            if self.scenario == Scenario::UnsupportedGame {
                Page::Unavailable
            } else {
                Page::Choice
            }
        } else {
            if self.view.lobby {
                Page::Lobby
            } else {
                Page::Mode
            }
        };
        self.navigate(page, PageDirection::Backward);
        self.view.selected = 0;
        self.view.refreshing = false;
        self.fault_delivered = false;
        self.set_phase(Phase::Idle);
    }
    pub fn refresh_rooms(&mut self) {
        self.view_revision = self.view_revision.wrapping_add(1);
        if !matches!(self.view.page, Page::Rooms | Page::Lobby | Page::Mode) {
            return;
        }
        self.view.refreshing = true;
        self.view.selected = 0;
    }
    pub fn switch_entry(&mut self) {
        self.return_to_launcher = false;
        self.view.source = match self.view.source {
            LaunchSource::Menu => LaunchSource::Game,
            LaunchSource::Game => LaunchSource::Menu,
        };
        self.view.lobby = self.view.source == LaunchSource::Menu;
        if self.view.source == LaunchSource::Game {
            self.launcher.close();
        } else {
            self.launcher.resume();
        }
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
    fn navigate(&mut self, page: Page, direction: PageDirection) {
        self.view_revision = self.view_revision.wrapping_add(1);
        self.view.page = page;
        self.direction = direction;
    }
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
        self.view_revision = self.view_revision.wrapping_add(1);
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
    fn exit(&mut self, outcome: Outcome, direction: PageDirection) {
        self.view.outcome = outcome;
        self.view.busy = false;
        if self.return_to_launcher {
            self.return_to_launcher = false;
            self.launcher.resume();
            self.launcher.result(match outcome {
                Outcome::SinglePlayer => "LOCAL DEMO: single-player requested",
                Outcome::Multiplayer => "LOCAL DEMO: multiplayer requested",
                Outcome::KeepNetwork => "LOCAL DEMO: direct network retained",
                _ => "LOCAL DEMO: returned to launcher",
            });
            return;
        }
        self.navigate(Page::Outcome, direction);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub lobby: bool,
    pub return_to_launcher: bool,
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
    pub(super) fn selected_title(&self) -> alloc::string::String {
        self.view_revision();
        self.selected();
        self.selected_title_value()
    }
    pub(super) fn heading(&self) -> alloc::string::String {
        self.view_revision();
        self.heading_value()
    }
    pub(super) fn devices(&self) -> alloc::vec::Vec<NetworkPeer> {
        self.view_revision();
        self.devices_value()
    }
    pub(super) fn room_name(&self) -> alloc::string::String {
        self.view_revision();
        self.room_name_value()
    }
    pub(super) fn subtitle(&self) -> alloc::string::String {
        if self.launcher().active() {
            return "本机游戏".into();
        }
        self.view_revision();
        self.subtitle_value()
    }
    pub(super) fn library_counter(&self) -> Option<(usize, usize)> {
        if self.launcher().active() {
            return self.launcher().shelf_counter();
        }
        self.view_revision();
        self.library_counter_value()
    }
    pub(super) fn can_switch_theme(&self) -> bool {
        if self.launcher().active() {
            return false;
        }
        let view = self.presentation();
        matches!(view.page, Page::Mode | Page::Choice) && !view.busy
    }
    pub(super) fn action_enabled(&self) -> bool {
        !copy::confirm_text(&self.presentation()).is_empty()
    }
    pub(super) fn status(&self) -> &'static str {
        copy::status_text(&self.presentation())
    }
    pub(super) fn hint(&self) -> &'static str {
        if self.launcher().active() {
            return self.launcher().browser_hint();
        }
        copy::hint_text(&self.presentation())
    }
    pub(super) fn back(&self) -> &'static str {
        if self.launcher().active() {
            return self.launcher().browser_back();
        }
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
        if self.launcher().active() {
            return self.launcher().browser_confirm();
        }
        copy::confirm_text(&self.presentation())
    }
    pub(super) fn confirm_visible(&self) -> bool {
        !self.confirm_label().is_empty()
    }
    pub(super) fn text(&self, field: TextField) -> alloc::string::String {
        let text = match field {
            TextField::Headline => self.headline(),
            TextField::Next => self.next(),
            TextField::HostButton => self.host_button(),
            TextField::Peer => self.peer(),
            TextField::RoomName => return self.room_name(),
        };
        text.into()
    }
}
