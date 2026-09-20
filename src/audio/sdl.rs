use alloc::string::String;

use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};

use super::{AudioBank, AudioCommand, AudioMixer, AudioOutputState, AudioSink};

struct MixerCallback {
    mixer: AudioMixer,
    channels: u8,
}

impl AudioCallback for MixerCallback {
    type Channel = i16;

    fn callback(&mut self, output: &mut [i16]) {
        self.mixer.render_interleaved(output, self.channels);
    }
}

pub struct SdlAudioSink {
    sdl: Option<sdl2::Sdl>,
    device: Option<AudioDevice<MixerCallback>>,
    state: AudioOutputState,
}

impl Default for SdlAudioSink {
    fn default() -> Self {
        Self::new()
    }
}

impl SdlAudioSink {
    pub const fn new() -> Self {
        Self {
            sdl: None,
            device: None,
            state: AudioOutputState::Starting,
        }
    }
}

impl AudioSink for SdlAudioSink {
    type Error = String;

    fn start(&mut self, bank: &'static AudioBank) -> Result<(), Self::Error> {
        let sdl = sdl2::init()?;
        let audio = sdl.audio()?;
        let desired = AudioSpecDesired {
            freq: Some(48_000),
            channels: Some(2),
            samples: Some(512),
        };
        let device = audio.open_playback(None, &desired, |spec| MixerCallback {
            mixer: AudioMixer::new(bank, spec.freq.max(1) as u32)
                .expect("SDL supplies a positive sample rate"),
            channels: spec.channels.max(1),
        })?;
        device.resume();
        self.device = Some(device);
        self.sdl = Some(sdl);
        self.state = AudioOutputState::Ready;
        Ok(())
    }

    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error> {
        let Some(device) = &mut self.device else {
            return Err("SDL audio device is not started".into());
        };
        device
            .lock()
            .mixer
            .apply(command)
            .map_err(|error| alloc::format!("audio mixer error: {error:?}"))
    }

    fn suspend(&mut self) {
        if let Some(device) = &self.device {
            device.pause();
        }
        self.state = AudioOutputState::Suspended;
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        let Some(device) = &self.device else {
            return Err("SDL audio device is not started".into());
        };
        device.resume();
        self.state = AudioOutputState::Ready;
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(device) = &mut self.device {
            device.pause();
            let _ = device.lock().mixer.apply(AudioCommand::StopAll);
        }
        self.state = AudioOutputState::Stopped;
    }

    fn state(&self) -> AudioOutputState {
        self.state
    }
}
