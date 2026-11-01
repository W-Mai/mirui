use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    Lobby,
    Candidates,
    Mode,
    #[default]
    Choice,
    Host,
    Rooms,
    Joined,
    Leaving,
    Unavailable,
    Games,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchSource {
    #[default]
    Menu,
    Game,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    #[default]
    System,
    SinglePlayer,
    Multiplayer,
    KeepNetwork,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Normal,
    ConnectionFailure,
    RestoreFailure,
    ExistingRoom,
    EmptyRooms,
    Reconnect,
    PeerClosed,
    EmptyGames,
    UnsupportedGame,
}

impl Scenario {
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::ConnectionFailure,
            Self::ConnectionFailure => Self::RestoreFailure,
            Self::RestoreFailure => Self::ExistingRoom,
            Self::ExistingRoom => Self::EmptyRooms,
            Self::EmptyRooms => Self::Reconnect,
            Self::Reconnect => Self::PeerClosed,
            Self::PeerClosed => Self::EmptyGames,
            Self::EmptyGames => Self::UnsupportedGame,
            Self::UnsupportedGame => Self::Normal,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Case: normal",
            Self::ConnectionFailure => "Case: fail",
            Self::RestoreFailure => "Case: restore",
            Self::ExistingRoom => "Case: occupied",
            Self::EmptyRooms => "Case: empty",
            Self::Reconnect => "Case: reconnect",
            Self::PeerClosed => "Case: closed",
            Self::EmptyGames => "Case: no games",
            Self::UnsupportedGame => "Case: unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    ResolvingGame,
    SelectingGame,
    #[default]
    Idle,
    Creating,
    WaitingPeer,
    PeerPreparing,
    ApprovalPending,
    HostReady,
    Connecting,
    Checking,
    Notifying,
    WaitingStart,
    Starting,
    Leaving,
    Closed,
    Failed,
    RestoreFailed,
    RoomActive,
    NetworkConfirming,
    NetworkConnected,
    NetworkReconnecting,
    ApprovalPendingClient,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomRow {
    pub name: String,
    pub detail: String,
    pub signal: u8,
    pub available: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub lobby: bool,
    pub source: LaunchSource,
    pub outcome: Outcome,
    pub network_only: bool,
    pub connected: bool,
    pub local_address: String,
    pub peers: Vec<NetworkPeer>,
    pub page: Page,
    pub title: String,
    pub core: String,
    pub selected: usize,
    pub rooms: Vec<RoomRow>,
    pub room_name: String,
    pub ready: bool,
    pub busy: bool,
    pub refreshing: bool,
    pub status: String,
    pub phase: Phase,
    pub back_label: String,
    pub entries: Vec<GameRow>,
}

impl Snapshot {
    pub(super) fn scene_page(&self) -> Page {
        if self.lobby && self.page == Page::Mode {
            Page::Lobby
        } else {
            self.page
        }
    }
    pub(super) fn presentation(&self) -> super::model::Presentation {
        super::model::Presentation {
            lobby: self.lobby,
            return_to_launcher: false,
            source: self.source,
            outcome: self.outcome,
            page: self.page,
            phase: self.phase,
            network_only: self.network_only,
            connected: self.connected,
            ready: self.ready,
            busy: self.busy,
            refreshing: self.refreshing,
            rooms_empty: self.rooms.is_empty(),
            entries_empty: self.entries.is_empty(),
            selected_room_available: self
                .rooms
                .get(self.selected)
                .is_some_and(|row| row.available),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkPeer {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameRow {
    pub key: u32,
    pub platform: usize,
    pub name: String,
    pub detail: String,
    pub directory: bool,
}
