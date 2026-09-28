use super::style::{BLUE, MINT, SURFACE, TEXT_MUTED, VIOLET};
use crate::core::reactive::Signal;
use crate::prelude::{ColorToken, Fixed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleMode {
    Orbit,
    Flow,
    Pulse,
}

impl ConsoleMode {
    pub(super) const fn accent(self) -> ColorToken {
        match self {
            Self::Orbit => MINT,
            Self::Flow => BLUE,
            Self::Pulse => VIOLET,
        }
    }

    pub(super) fn chip_background(self, selected: Self) -> ColorToken {
        if self == selected {
            self.accent()
        } else {
            SURFACE
        }
    }

    pub(super) fn chip_foreground(self, selected: Self) -> ColorToken {
        if self == selected {
            match self {
                Self::Orbit => ColorToken::OnPrimary,
                Self::Flow => ColorToken::OnSecondary,
                Self::Pulse => ColorToken::OnTertiary,
            }
        } else {
            TEXT_MUTED
        }
    }

    fn phase_delta(self, elapsed_ms: u32) -> Fixed {
        let elapsed = elapsed_ms as i32;
        match self {
            Self::Orbit => Fixed::from_ratio(elapsed * 3, 50),
            Self::Flow => Fixed::from_ratio(elapsed * 9, 100),
            Self::Pulse => Fixed::from_ratio(elapsed * 3, 25),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleState {
    pub mode: ConsoleMode,
    pub intensity: u8,
    pub focused_node: u8,
    pub paused: bool,
    phase: Fixed,
    revision: u32,
}

impl ConsoleState {
    pub const fn live() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 68,
            focused_node: 1,
            paused: false,
            phase: Fixed::ZERO,
            revision: 0,
        }
    }

    pub const fn capture() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 78,
            focused_node: 1,
            paused: false,
            phase: Fixed::from_int(32),
            revision: 0,
        }
    }

    pub const fn phase(&self) -> Fixed {
        self.phase
    }

    pub const fn revision(&self) -> u32 {
        self.revision
    }

    fn select_mode(&mut self, mode: ConsoleMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn cycle_focus(&mut self) {
        self.focused_node = (self.focused_node + 1) % 3;
        self.revision = self.revision.wrapping_add(1);
    }

    fn set_intensity(&mut self, value: Fixed) -> bool {
        let next = value.to_int().clamp(0, 100) as u8;
        if self.intensity == next {
            return false;
        }
        self.intensity = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(super) fn toggle_paused(&mut self) {
        self.paused = !self.paused;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn advance(&mut self, delta_ms: u16) -> bool {
        if self.paused || delta_ms == 0 {
            return false;
        }
        let elapsed = u32::from(delta_ms.min(100));
        self.phase += self.mode.phase_delta(elapsed);
        while self.phase >= Fixed::from_int(360) {
            self.phase -= Fixed::from_int(360);
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoRunMode {
    Live,
    Capture,
}

#[derive(Clone)]
pub(super) struct ConsoleModel {
    state: Signal<ConsoleState>,
}

impl ConsoleModel {
    #[cfg(any(feature = "std", test))]
    pub(super) fn new(state: ConsoleState) -> Self {
        Self {
            state: Signal::new(state),
        }
    }

    pub(super) fn signal(&self) -> Signal<ConsoleState> {
        self.state.clone()
    }

    pub(super) fn snapshot(&self) -> ConsoleState {
        self.state.get_untracked()
    }

    pub(super) fn advance(&self, delta_ms: u16) {
        let mut state = self.snapshot();
        if state.advance(delta_ms) {
            self.state.set(state);
        }
    }
}

pub(super) enum ConsoleAction {
    SelectMode(ConsoleMode),
    CycleFocus,
    SetIntensity(Fixed),
    TogglePaused,
}

impl ConsoleAction {
    pub(super) fn publish(self, signal: &Signal<ConsoleState>) {
        let mut state = signal.get_untracked();
        let changed = match self {
            Self::SelectMode(mode) => state.select_mode(mode),
            Self::CycleFocus => {
                state.cycle_focus();
                true
            }
            Self::SetIntensity(value) => state.set_intensity(value),
            Self::TogglePaused => {
                state.toggle_paused();
                true
            }
        };
        if changed {
            signal.set(state);
        }
    }
}
