use crate::types::Fixed64;

pub(crate) const MAX_NODES: usize = 3;
pub(crate) const MAX_TRAIL: usize = 192;
pub(crate) const MAX_TELEMETRY: usize = 96;
pub(crate) const MAX_EVENTS: usize = 12;
pub(crate) const MAX_PREVIEW: usize = 240;
/// Events listed on the record page, newest last.
pub(crate) const LOG_LINES: usize = 6;

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

/// A recorded or predicted position in quarter units, small enough to keep
/// a few hundred of them on the model.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TrackPoint {
    pub(crate) x: i16,
    pub(crate) y: i16,
}

impl TrackPoint {
    pub(crate) fn from_point(point: OrbitPoint) -> Self {
        let quarter = |value: Fixed64| (value * 4).to_int().clamp(-32_000, 32_000) as i16;
        Self {
            x: quarter(point.x),
            y: quarter(point.y),
        }
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
    Brief,
    /// Leave the flight in progress for this mission.
    Confirm(u8),
    Result,
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
    Fuel,
    Heat,
    Terminal,
    InvalidDelay,
    QueueFull,
    MissingNode,
    OutsideWindow,
    /// Contact is lost: only nodes planned before it still fire.
    NoContact,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ManeuverNode {
    pub(crate) id: u8,
    pub(crate) at: Fixed64,
    pub(crate) dv_tenths: u8,
    pub(crate) angle_degrees: i16,
}

/// Distance and speed in tenths, sampled for the record charts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitSample {
    pub(crate) radius_tenths: u16,
    pub(crate) speed_tenths: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitEventKind {
    Loaded,
    /// A manual burn of this many tenths.
    Burn(u8),
    /// A planned node fired with this many tenths.
    NodeBurn(u8),
    NodeSkipped(OrbitError),
    /// A goal was sampled; the number of goals now done.
    Goal(u8),
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
