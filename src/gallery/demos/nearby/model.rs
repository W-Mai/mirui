use super::{copy, data::*, style::*};
use crate::prelude::*;
use alloc::{string::ToString, vec};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PageKey {
    pub page: Page,
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
        }
    }
}

#[model]
impl DisplayState {
    pub fn snapshot(&self) -> Snapshot {
        self.view.clone()
    }
    #[observe]
    pub fn page_key(&self) -> PageKey {
        PageKey {
            page: self.view.page,
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
            page: self.view.page,
            phase: self.view.phase,
            network_only: self.view.network_only,
            connected: self.view.connected,
            ready: self.view.ready,
            busy: self.view.busy,
            refreshing: self.view.refreshing,
            rooms_empty: self.view.rooms.is_empty(),
            entries_empty: self.view.entries.is_empty(),
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
        self.view.selected = index.min(3);
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
                let Some(game) = self.view.entries.get(self.view.selected) else {
                    return;
                };
                self.view.title.clone_from(&game.name);
                self.view.page = if self.view.selected == 2 {
                    Page::Unavailable
                } else {
                    Page::Choice
                };
            }
            Page::Choice => {
                self.view.page = if self.view.selected == 0 {
                    Page::Host
                } else {
                    Page::Rooms
                };
                self.view.phase = if self.view.selected == 0 {
                    Phase::WaitingPeer
                } else {
                    Phase::Idle
                };
            }
            Page::Rooms => {
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
                self.view.phase = Phase::Connecting;
            }
            Page::Host | Page::Joined => {
                self.advance();
                return;
            }
            Page::Leaving | Page::Unavailable => {
                self.back_page();
                return;
            }
        }
        self.view.selected = 0;
    }
    pub fn advance(&mut self) {
        self.view.phase = match self.view.phase {
            Phase::WaitingPeer => Phase::PeerPreparing,
            Phase::PeerPreparing => Phase::ApprovalPending,
            Phase::ApprovalPending if self.view.network_only => Phase::NetworkConnected,
            Phase::ApprovalPending => Phase::HostReady,
            Phase::Connecting => Phase::Checking,
            Phase::Checking => Phase::Notifying,
            Phase::Notifying if self.view.network_only => Phase::NetworkConnected,
            Phase::Notifying => Phase::WaitingStart,
            Phase::HostReady | Phase::WaitingStart => Phase::Starting,
            Phase::Starting | Phase::NetworkConnected => {
                self.view.page = Page::Leaving;
                Phase::Leaving
            }
            Phase::Leaving => {
                self.back_page();
                return;
            }
            _ => return,
        };
        self.view.ready = matches!(
            self.view.phase,
            Phase::HostReady | Phase::WaitingStart | Phase::Starting
        );
        self.view.connected = self.view.phase == Phase::NetworkConnected;
        if self.view.connected {
            self.view.local_address = "192.168.4.1".into();
            self.view.peers = vec![NetworkPeer {
                name: "好友".into(),
                address: "192.168.4.2".into(),
            }];
        }
    }
    pub fn back_page(&mut self) {
        self.view.page = match self.view.page {
            Page::Mode | Page::Games => Page::Mode,
            Page::Choice if self.view.network_only => Page::Mode,
            Page::Choice | Page::Unavailable => Page::Games,
            Page::Rooms | Page::Leaving => Page::Choice,
            Page::Host | Page::Joined => Page::Leaving,
        };
        self.view.phase = if self.view.page == Page::Leaving {
            Phase::Leaving
        } else {
            Phase::Idle
        };
        self.view.selected = 0;
        self.view.ready = false;
        self.view.connected = false;
        self.view.local_address.clear();
        self.view.peers.clear();
    }
    pub fn restart(&mut self) {
        self.back_page();
        self.view.page = Page::Mode;
        self.view.phase = Phase::Idle;
    }
    pub fn refresh_rooms(&mut self) {
        if self.view.page != Page::Rooms {
            return;
        }
        self.view.rooms.rotate_left(1);
        self.view.selected = 0;
        self.refresh = self.refresh.wrapping_add(1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
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
        "返回"
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
