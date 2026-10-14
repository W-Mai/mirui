use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
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
pub enum Phase {
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkPeer {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameRow {
    pub name: String,
    pub detail: String,
    pub directory: bool,
}
