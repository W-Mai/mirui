use super::ConsoleMode;
use super::style::{BLUE, MINT, SURFACE, TEXT_MUTED, VIOLET};
use crate::prelude::{ColorToken, Fixed};

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

#[crate::model]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsoleState {
    pub mode: ConsoleMode,
    pub intensity: u8,
    pub focused_node: u8,
    pub paused: bool,
    phase: Fixed,
}

impl ConsoleState {
    pub const fn live() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 68,
            focused_node: 1,
            paused: false,
            phase: Fixed::ZERO,
        }
    }

    pub const fn capture() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 78,
            focused_node: 1,
            paused: false,
            phase: Fixed::from_int(32),
        }
    }
}

#[crate::model]
impl ConsoleState {
    #[observe]
    pub fn phase(&self) -> Fixed {
        self.phase
    }

    #[observe]
    pub(super) fn mode(&self) -> ConsoleMode {
        self.mode
    }

    #[observe]
    pub(super) fn intensity(&self) -> u8 {
        self.intensity
    }

    #[observe]
    pub(super) fn focused_node(&self) -> u8 {
        self.focused_node
    }

    #[observe]
    pub(super) fn paused(&self) -> bool {
        self.paused
    }

    pub(super) fn select_mode(&mut self, mode: ConsoleMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
    }

    pub(super) fn cycle_focus(&mut self) {
        self.focused_node = (self.focused_node + 1) % 3;
    }

    pub(super) fn set_intensity(&mut self, value: Fixed) {
        let next = value.to_int().clamp(0, 100) as u8;
        if self.intensity == next {
            return;
        }
        self.intensity = next;
    }

    pub(super) fn toggle_paused(&mut self) {
        self.paused = !self.paused;
    }

    pub(super) fn advance(&mut self, delta_ms: u16) {
        if self.paused || delta_ms == 0 {
            return;
        }
        let elapsed = u32::from(delta_ms.min(100));
        self.phase += self.mode.phase_delta(elapsed);
        while self.phase >= Fixed::from_int(360) {
            self.phase -= Fixed::from_int(360);
        }
    }
}
