use crate::types::Fixed64;

pub(crate) const MAX_NODES: usize = 3;
pub(crate) const MAX_TRAIL: usize = 192;
pub(crate) const MAX_TELEMETRY: usize = 80;
pub(crate) const MAX_EVENTS: usize = 12;
pub(crate) const MAX_PREVIEW: usize = 240;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitPoint {
    pub(crate) x: Fixed64,
    pub(crate) y: Fixed64,
}

impl OrbitPoint {
    pub(super) fn radius(self) -> Fixed64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitBody {
    pub(crate) position: OrbitPoint,
    pub(crate) velocity: OrbitPoint,
}

impl OrbitBody {
    pub(crate) fn radius(self) -> Fixed64 {
        self.position.radius()
    }

    pub(crate) fn speed(self) -> Fixed64 {
        self.velocity.radius()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitPage {
    Map,
    Plan,
    Record,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitModal {
    None,
    Missions,
    Confirm(u8),
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitStatus {
    Ready,
    Running,
    Paused,
    Won,
    Crashed,
    Escaped,
}

impl OrbitStatus {
    pub(crate) const fn terminal(self) -> bool {
        matches!(self, Self::Won | Self::Crashed | Self::Escaped)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitError {
    InvalidBurn,
    Fuel,
    Heat,
    Terminal,
    InvalidDelay,
    QueueFull,
    MissingNode,
    OutsideWindow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ManeuverNode {
    pub(crate) id: u8,
    pub(crate) at: Fixed64,
    pub(crate) dv_tenths: u8,
    pub(crate) angle_degrees: i16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitTelemetry {
    pub(crate) time: Fixed64,
    pub(crate) radius: Fixed64,
    pub(crate) speed: Fixed64,
    pub(crate) fuel: Fixed64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitEventKind {
    Loaded,
    Burn,
    NodeSkipped,
    Goal,
    Won,
    Crashed,
    Escaped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OrbitEvent {
    pub(crate) time: Fixed64,
    pub(crate) kind: OrbitEventKind,
}

impl Default for OrbitEvent {
    fn default() -> Self {
        Self {
            time: Fixed64::ZERO,
            kind: OrbitEventKind::Loaded,
        }
    }
}
