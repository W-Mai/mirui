#[cfg(feature = "audio")]
use crate::audio::{AudioHandle, AudioOutputState, AudioStateSignal, AudioTone, Waveform};
#[cfg(feature = "audio")]
use crate::gallery::play::audio::MARBLE_AUDIO_BANK;
#[cfg(feature = "audio")]
use crate::gallery::play::marble::{MarbleSound, PadTimbre};

#[cfg(feature = "audio")]
pub(super) fn audio_label(state: &Option<AudioStateSignal>) -> &'static str {
    if let Some(state) = state.as_ref().map(AudioStateSignal::get) {
        return if state.muted {
            "SOUND"
        } else if state.output == AudioOutputState::Ready {
            "ON"
        } else {
            "WAIT"
        };
    }
    "SOUND"
}

#[cfg(not(feature = "audio"))]
pub(super) fn audio_label(_: &bool) -> &'static str {
    "SOUND"
}

#[cfg(feature = "audio")]
pub(super) fn audio_visible(state: &Option<AudioStateSignal>) -> bool {
    state.is_some()
}

#[cfg(not(feature = "audio"))]
pub(super) fn audio_visible(available: &bool) -> bool {
    *available
}

#[cfg(feature = "audio")]
fn scaled_gain(gain: u8, scale: u8) -> u8 {
    (u16::from(gain) * u16::from(scale) / 255) as u8
}

#[cfg(feature = "audio")]
pub(super) fn submit_pad_sound(
    audio: &AudioHandle,
    pitch: u8,
    timbre: PadTimbre,
    gain: u8,
    delay_ms: u16,
) {
    let tone = |pitch, waveform, duration_ms, scale| {
        let _ = audio.tone(
            AudioTone::new(pitch, waveform, duration_ms, scaled_gain(gain, scale))
                .with_delay_ms(delay_ms),
        );
    };
    match timbre {
        PadTimbre::Mallet => {
            tone(pitch, Waveform::Sine, 720, 225);
            tone(pitch.saturating_add(17).min(127), Waveform::Sine, 190, 47);
        }
        PadTimbre::Synth => {
            tone(pitch, Waveform::Sine, 950, 190);
            tone(pitch, Waveform::Triangle, 700, 54);
        }
        PadTimbre::Bass => {
            let bass = pitch.saturating_sub(12);
            tone(bass, Waveform::Sine, 360, 230);
            tone(bass, Waveform::Triangle, 280, 43);
        }
        PadTimbre::Drum => {
            tone(48, Waveform::Sine, 180, 220);
            tone(41, Waveform::Triangle, 120, 72);
        }
    }
}

#[cfg(feature = "audio")]
pub(super) fn submit_sound(audio: &AudioHandle, sound: MarbleSound) {
    let MarbleSound::Pad {
        pitch,
        timbre,
        gain,
        delay_ms,
        ..
    } = sound;
    submit_pad_sound(audio, pitch, timbre, gain, delay_ms);
}

#[cfg(feature = "audio")]
pub(super) fn audio_bank() -> &'static crate::audio::AudioBank {
    &MARBLE_AUDIO_BANK
}
