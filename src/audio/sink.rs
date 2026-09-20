use core::convert::Infallible;

use super::{AudioBank, AudioCommand, AudioOutputState};

pub trait AudioSink {
    type Error;

    fn start(&mut self, bank: &'static AudioBank) -> Result<(), Self::Error>;

    fn unlock(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn update(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error>;

    fn suspend(&mut self) {}

    fn resume(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn stop(&mut self) {}

    fn state(&self) -> AudioOutputState {
        AudioOutputState::Ready
    }
}

#[derive(Default)]
pub struct SilentAudioSink {
    state: AudioOutputState,
}

impl AudioSink for SilentAudioSink {
    type Error = Infallible;

    fn start(&mut self, _bank: &'static AudioBank) -> Result<(), Self::Error> {
        self.state = AudioOutputState::Ready;
        Ok(())
    }

    fn submit(&mut self, _command: AudioCommand) -> Result<(), Self::Error> {
        Ok(())
    }

    fn suspend(&mut self) {
        self.state = AudioOutputState::Suspended;
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        self.state = AudioOutputState::Ready;
        Ok(())
    }

    fn stop(&mut self) {
        self.state = AudioOutputState::Stopped;
    }

    fn state(&self) -> AudioOutputState {
        self.state
    }
}
