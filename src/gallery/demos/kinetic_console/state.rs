use super::style::{AMBER, BLUE, CYAN, PINK, VIOLET};
use crate::core::reactive::Signal;
use crate::prelude::{ColorToken, Entity, Fixed};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConsoleState {
    pub(super) mode: ConsoleMode,
    pub(super) intensity: Fixed,
    pub(super) focused: u8,
    pub(super) paused: bool,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: Fixed::from_int(68),
            focused: 0,
            paused: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct ConsoleModel {
    pub(super) mode: Signal<ConsoleMode>,
    pub(super) intensity: Signal<Fixed>,
    pub(super) focused: Signal<u8>,
    pub(super) paused: Signal<bool>,
}

impl Default for ConsoleModel {
    fn default() -> Self {
        let state = ConsoleState::default();
        Self {
            mode: Signal::new(state.mode),
            intensity: Signal::new(state.intensity),
            focused: Signal::new(state.focused),
            paused: Signal::new(state.paused),
        }
    }
}

impl ConsoleModel {
    pub(super) fn snapshot(&self) -> ConsoleState {
        ConsoleState {
            mode: self.mode.get_untracked(),
            intensity: self.intensity.get_untracked(),
            focused: self.focused.get_untracked(),
            paused: self.paused.get_untracked(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct ConsoleMotion {
    pub(super) phase: super::super::motion::BrakedPhase,
    pub(super) state: ConsoleState,
    pub(super) wave_elapsed_ms: u16,
}

pub(super) struct ConsoleNodes {
    pub(super) orbit: Entity,
    pub(super) wave: Entity,
}

pub(super) enum ConsoleAction {
    Select(ConsoleMode),
    SetIntensity(Fixed),
    CycleFocus,
    TogglePaused,
}

impl ConsoleAction {
    pub(super) fn publish(self, model: &ConsoleModel) {
        match self {
            Self::Select(mode) => {
                if mode != model.mode.get_untracked() {
                    model.mode.set(mode);
                }
            }
            Self::SetIntensity(value) => {
                let current = model.intensity.get_untracked();
                let next = value.clamp(Fixed::ZERO, Fixed::from_int(100));
                if next != current {
                    model.intensity.set(next);
                }
            }
            Self::CycleFocus => model
                .focused
                .update(|focused| *focused = (*focused + 1) % 3),
            Self::TogglePaused => model.paused.update(|paused| *paused = !*paused),
        }
    }
}
