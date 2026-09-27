use alloc::rc::{Rc, Weak};
use core::cell::RefCell;

use super::{
    AudioBus, AudioCommand, AudioOutputState, AudioState, AudioStateSignal, AudioTone, CueId,
};
use crate::core::model::{BindType, SharedValue};

trait AudioControl {
    fn submit(&self, command: AudioCommand) -> bool;
    fn state(&self) -> AudioState;
    fn state_signal(&self) -> AudioStateSignal;
}

/// A shared command entry point for the audio output installed on an app.
#[derive(Clone)]
pub struct AudioHandle(Weak<dyn AudioControl>);

impl AudioHandle {
    pub fn play(&self, cue: CueId) -> bool {
        self.submit(AudioCommand::Play { cue, gain: u8::MAX })
    }

    pub fn play_with_gain(&self, cue: CueId, gain: u8) -> bool {
        self.submit(AudioCommand::Play { cue, gain })
    }

    pub fn tone(&self, tone: AudioTone) -> bool {
        self.submit(AudioCommand::Tone(tone))
    }

    pub fn stop(&self, cue: CueId) -> bool {
        self.submit(AudioCommand::Stop { cue })
    }

    pub fn stop_all(&self) -> bool {
        self.submit(AudioCommand::StopAll)
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        self.submit(AudioCommand::SetMuted(muted))
    }

    pub fn toggle_muted(&self) -> bool {
        self.state()
            .is_some_and(|state| self.set_muted(!state.muted))
    }

    pub fn set_master_gain(&self, gain: u8) -> bool {
        self.submit(AudioCommand::SetMasterGain(gain))
    }

    pub fn state(&self) -> Option<AudioState> {
        self.0.upgrade().map(|control| control.state())
    }

    /// Returns the reactive state source for properties that follow audio changes.
    ///
    /// Call [`AudioStateSignal::get`] while evaluating a reactive property to subscribe.
    /// Returns `None` after the owning app is dropped.
    pub fn state_signal(&self) -> Option<AudioStateSignal> {
        self.0.upgrade().map(|control| control.state_signal())
    }

    fn submit(&self, command: AudioCommand) -> bool {
        self.0
            .upgrade()
            .is_some_and(|control| control.submit(command))
    }
}

impl BindType for AudioHandle {
    type Shared = Self;
}

impl SharedValue for AudioHandle {}

pub(crate) struct SharedAudioCore<const N: usize> {
    bus: RefCell<AudioBus<N>>,
    state: AudioStateSignal,
}

impl<const N: usize> SharedAudioCore<N> {
    pub(crate) fn new(bus: AudioBus<N>) -> Rc<Self> {
        let state = AudioStateSignal::new(AudioState::from_bus(&bus));
        Rc::new(Self {
            bus: RefCell::new(bus),
            state,
        })
    }

    pub(crate) fn handle(core: &Rc<Self>) -> AudioHandle {
        let erased: Rc<dyn AudioControl> = core.clone();
        AudioHandle(Rc::downgrade(&erased))
    }

    pub(crate) fn state_signal(&self) -> AudioStateSignal {
        self.state.clone()
    }

    pub(crate) fn set_output_state(&self, state: AudioOutputState) {
        self.bus.borrow_mut().set_state(state);
        self.publish_state();
    }

    pub(crate) fn record_failure(&self) {
        self.bus.borrow_mut().record_failure();
        self.publish_state();
    }

    pub(crate) fn pop(&self) -> Option<AudioCommand> {
        self.bus.borrow_mut().pop()
    }

    fn publish_state(&self) {
        let state = AudioState::from_bus(&self.bus.borrow());
        self.state.publish(state);
    }
}

impl<const N: usize> AudioControl for SharedAudioCore<N> {
    fn submit(&self, command: AudioCommand) -> bool {
        let accepted = {
            let mut bus = self.bus.borrow_mut();
            match command {
                AudioCommand::Play { cue, gain } => bus.play_with_gain(cue, gain),
                AudioCommand::Tone(tone) => bus.tone(tone),
                AudioCommand::Stop { cue } => bus.stop(cue),
                AudioCommand::StopAll => bus.stop_all(),
                AudioCommand::SetMasterGain(gain) => bus.set_master_gain(gain),
                AudioCommand::SetMuted(muted) => bus.set_muted(muted),
            }
        };
        if accepted {
            self.publish_state();
        }
        accepted
    }

    fn state(&self) -> AudioState {
        AudioState::from_bus(&self.bus.borrow())
    }

    fn state_signal(&self) -> AudioStateSignal {
        self.state.clone()
    }
}
