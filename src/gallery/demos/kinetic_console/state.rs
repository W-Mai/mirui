use super::style::{AMBER, BLUE, CYAN, PINK, VIOLET};
use crate::gallery::play::change::ChangeSet;
use crate::prelude::{ColorToken, Fixed};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ConsoleMode {
    #[default]
    Orbit,
    Flow,
    Pulse,
}

impl ConsoleMode {
    pub(super) const fn accent(self) -> ColorToken {
        match self {
            Self::Orbit => CYAN,
            Self::Flow => VIOLET,
            Self::Pulse => AMBER,
        }
    }

    pub(super) const fn secondary(self) -> ColorToken {
        match self {
            Self::Orbit => BLUE,
            Self::Flow => CYAN,
            Self::Pulse => PINK,
        }
    }

    pub(super) const fn speed(self) -> i32 {
        match self {
            Self::Orbit => 34,
            Self::Flow => 48,
            Self::Pulse => 62,
        }
    }
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConsoleModel {
    pub(super) mode: ConsoleMode,
    pub(super) intensity: Fixed,
    pub(super) focused: u8,
    pub(super) paused: bool,
}

impl Default for ConsoleModel {
    fn default() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: Fixed::from_int(68),
            focused: 0,
            paused: false,
        }
    }
}

#[crate::model]
impl ConsoleModel {
    #[observe]
    pub(super) fn mode(&self) -> ConsoleMode {
        self.mode
    }

    #[observe]
    pub(super) fn intensity(&self) -> Fixed {
        self.intensity
    }

    #[observe]
    pub(super) fn focused(&self) -> u8 {
        self.focused
    }

    #[observe]
    pub(super) fn paused(&self) -> bool {
        self.paused
    }

    pub(super) fn select_mode(&mut self, mode: ConsoleMode) -> ChangeSet {
        if self.mode == mode {
            return ChangeSet::NONE;
        }
        self.mode = mode;
        changed()
    }

    pub(super) fn set_intensity(&mut self, value: Fixed) -> ChangeSet {
        let next = value.clamp(Fixed::ZERO, Fixed::from_int(100));
        if self.intensity == next {
            return ChangeSet::NONE;
        }
        self.intensity = next;
        changed()
    }

    pub(super) fn cycle_focus(&mut self) -> ChangeSet {
        self.focused = (self.focused + 1) % 3;
        changed()
    }

    pub(super) fn toggle_paused(&mut self) -> ChangeSet {
        self.paused = !self.paused;
        changed()
    }
}

const fn changed() -> ChangeSet {
    ChangeSet::MODEL.union(ChangeSet::VISUAL)
}

#[derive(Clone, Copy, Default)]
pub(super) struct ConsoleMotion {
    pub(super) phase: super::super::motion::BrakedPhase,
    wave_phase: Fixed,
    wave_elapsed_ms: u16,
}

impl ConsoleMotion {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 450;
    const WAVE_STEP_MS: u16 = 64;

    pub(super) fn advance(
        &mut self,
        mode: ConsoleMode,
        intensity: Fixed,
        paused: bool,
        delta_ms: u16,
    ) -> bool {
        let dt = delta_ms.min(Self::MAX_STEP_MS);
        if dt == 0 {
            return false;
        }
        let previous_phase = self.phase.phase();
        let speed = Fixed::from_int(mode.speed()) + intensity * Fixed::from_ratio(3, 5);
        let phase = self.phase.advance(
            dt,
            Self::RATE_RAMP_MS,
            speed,
            Fixed::ONE,
            paused,
            Fixed::from_int(360),
        );
        if phase != previous_phase {
            self.wave_elapsed_ms = self.wave_elapsed_ms.saturating_add(dt);
            if self.wave_elapsed_ms >= Self::WAVE_STEP_MS {
                self.wave_elapsed_ms = 0;
                self.wave_phase = phase;
            }
        }
        phase != previous_phase
    }

    pub(super) fn orbit_render_key(self) -> u64 {
        phase_key(self.phase.phase())
    }

    pub(super) fn wave_render_key(self) -> u64 {
        phase_key(self.wave_phase)
    }
}

fn phase_key(phase: Fixed) -> u64 {
    u64::from((phase * Fixed::from_int(256)).to_int() as u32)
}
