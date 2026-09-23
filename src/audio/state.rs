use super::{AudioBus, AudioOutputState};
use crate::core::reactive::Signal;

/// UI-facing snapshot of audio output and controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioState {
    pub muted: bool,
    pub master_gain: u8,
    pub output: AudioOutputState,
}

impl AudioState {
    pub const fn from_bus<const N: usize>(bus: &AudioBus<N>) -> Self {
        Self {
            muted: bus.is_muted(),
            master_gain: bus.master_gain(),
            output: bus.state(),
        }
    }
}

/// Reactive audio state published by `AudioPlugin`.
#[derive(Clone)]
pub struct AudioStateSignal(Signal<AudioState>);

impl AudioStateSignal {
    pub(crate) fn new(initial: AudioState) -> Self {
        Self(Signal::new(initial))
    }

    pub fn get(&self) -> AudioState {
        self.0.get()
    }

    pub fn get_untracked(&self) -> AudioState {
        self.0.get_untracked()
    }

    pub(crate) fn publish(&self, next: AudioState) -> bool {
        if self.0.get_untracked() == next {
            return false;
        }
        self.0.set(next);
        true
    }
}
