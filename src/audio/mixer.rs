use super::{AudioBank, AudioCommand, AudioTone, CueId, Score, Waveform};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MixerError {
    InvalidSampleRate,
    UnknownCue(CueId),
}

#[derive(Clone, Copy)]
struct Voice {
    score: Option<&'static Score>,
    tone: Option<AudioTone>,
    cue: CueId,
    gain: u8,
    cursor: u64,
    note_index: usize,
    active_note: usize,
    phase: u32,
    noise: u32,
    serial: u32,
    delay: u64,
}

impl Voice {
    const EMPTY: Self = Self {
        score: None,
        tone: None,
        cue: CueId::new(0),
        gain: 0,
        cursor: 0,
        note_index: 0,
        active_note: usize::MAX,
        phase: 0,
        noise: 0x4f1b_9d23,
        serial: 0,
        delay: 0,
    };

    fn stop(&mut self) {
        *self = Self::EMPTY;
    }

    const fn is_active(&self) -> bool {
        self.score.is_some() || self.tone.is_some()
    }
}

pub struct AudioMixer<const V: usize = 8> {
    bank: &'static AudioBank,
    voices: [Voice; V],
    sample_rate: u32,
    master_gain: u8,
    muted: bool,
    serial: u32,
}

impl<const V: usize> AudioMixer<V> {
    pub fn new(bank: &'static AudioBank, sample_rate: u32) -> Result<Self, MixerError> {
        if sample_rate == 0 {
            return Err(MixerError::InvalidSampleRate);
        }
        Ok(Self {
            bank,
            voices: [Voice::EMPTY; V],
            sample_rate,
            master_gain: 220,
            muted: false,
            serial: 0,
        })
    }

    pub fn apply(&mut self, command: AudioCommand) -> Result<(), MixerError> {
        match command {
            AudioCommand::Play { cue, gain } => self.play(cue, gain),
            AudioCommand::Tone(tone) => {
                self.play_tone(tone);
                Ok(())
            }
            AudioCommand::Stop { cue } => {
                for voice in &mut self.voices {
                    if voice.score.is_some() && voice.cue == cue {
                        voice.stop();
                    }
                }
                Ok(())
            }
            AudioCommand::StopAll => {
                for voice in &mut self.voices {
                    voice.stop();
                }
                Ok(())
            }
            AudioCommand::SetMasterGain(gain) => {
                self.master_gain = gain;
                Ok(())
            }
            AudioCommand::SetMuted(muted) => {
                self.muted = muted;
                Ok(())
            }
        }
    }

    pub fn render_interleaved(&mut self, output: &mut [i16], channels: u8) {
        if channels == 0 {
            output.fill(0);
            return;
        }
        let channels = usize::from(channels);
        for frame in output.chunks_mut(channels) {
            let sample = self.next_sample();
            frame.fill(sample);
        }
    }

    pub fn render_stereo(&mut self, output: &mut [i16]) {
        self.render_interleaved(output, 2);
    }

    pub fn render_mono(&mut self, output: &mut [i16]) {
        self.render_interleaved(output, 1);
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.is_active()).count()
    }

    fn play(&mut self, cue: CueId, gain: u8) -> Result<(), MixerError> {
        let score = &self.bank.cue(cue).ok_or(MixerError::UnknownCue(cue))?.score;
        if score.looping
            && self
                .voices
                .iter()
                .any(|voice| voice.score.is_some() && voice.cue == cue)
        {
            return Ok(());
        }
        let Some(slot) = self.voice_slot() else {
            return Ok(());
        };
        self.serial = self.serial.wrapping_add(1);
        self.voices[slot] = Voice {
            score: Some(score),
            tone: None,
            cue,
            gain,
            cursor: 0,
            note_index: 0,
            active_note: usize::MAX,
            phase: 0,
            noise: 0x9e37_79b9 ^ u32::from(cue.get()),
            serial: self.serial,
            delay: 0,
        };
        Ok(())
    }

    fn play_tone(&mut self, tone: AudioTone) {
        let Some(slot) = self.voice_slot() else {
            return;
        };
        self.serial = self.serial.wrapping_add(1);
        self.voices[slot] = Voice {
            score: None,
            tone: Some(tone),
            cue: CueId::new(0),
            gain: u8::MAX,
            cursor: 0,
            note_index: 0,
            active_note: 0,
            phase: 0,
            noise: 0x9e37_79b9 ^ self.serial,
            serial: self.serial,
            delay: u64::from(tone.delay_ms) * u64::from(self.sample_rate) / 1_000,
        };
    }

    fn voice_slot(&self) -> Option<usize> {
        self.voices
            .iter()
            .position(|voice| !voice.is_active())
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .filter(|(_, voice)| {
                        voice.tone.is_some() || voice.score.is_some_and(|score| !score.looping)
                    })
                    .min_by_key(|(_, voice)| voice.serial)
                    .map(|(index, _)| index)
            })
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, voice)| voice.serial)
                    .map(|(index, _)| index)
            })
    }

    fn next_sample(&mut self) -> i16 {
        let mut mixed = 0_i64;
        for voice in &mut self.voices {
            mixed += i64::from(Self::voice_sample(voice, self.sample_rate));
        }
        if self.muted {
            return 0;
        }
        let scaled = mixed * i64::from(self.master_gain) / 255;
        let magnitude = scaled.abs();
        let threshold = 12_000_i64;
        let compressed = if magnitude > threshold {
            threshold + (magnitude - threshold) / 4
        } else {
            magnitude
        };
        (scaled.signum() * compressed).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
    }

    fn voice_sample(voice: &mut Voice, sample_rate: u32) -> i32 {
        if let Some(tone) = voice.tone {
            return Self::tone_sample(voice, tone, sample_rate);
        }
        let Some(score) = voice.score else {
            return 0;
        };
        if score.ticks_per_second == 0 || score.length_ticks == 0 || score.notes.is_empty() {
            voice.stop();
            return 0;
        }
        let length = Self::tick_sample(score.length_ticks, score.ticks_per_second, sample_rate);
        if voice.cursor >= length {
            if score.looping && length > 0 {
                voice.cursor %= length;
                voice.note_index = 0;
                voice.active_note = usize::MAX;
                voice.phase = 0;
            } else {
                voice.stop();
                return 0;
            }
        }
        while voice.note_index < score.notes.len() {
            let note = score.notes[voice.note_index];
            let end = Self::tick_sample(
                note.start_ticks.saturating_add(note.duration_ticks),
                score.ticks_per_second,
                sample_rate,
            );
            if voice.cursor < end {
                break;
            }
            voice.note_index += 1;
        }
        let mut output = 0;
        if let Some(note) = score.notes.get(voice.note_index).copied() {
            let start = Self::tick_sample(note.start_ticks, score.ticks_per_second, sample_rate);
            let end = Self::tick_sample(
                note.start_ticks.saturating_add(note.duration_ticks),
                score.ticks_per_second,
                sample_rate,
            );
            if voice.cursor >= start && voice.cursor < end {
                if voice.active_note != voice.note_index {
                    voice.active_note = voice.note_index;
                    voice.phase = 0;
                }
                let raw = Self::wave_sample(note.waveform, voice);
                let envelope = Self::envelope(voice.cursor - start, end - start, sample_rate);
                output = (i64::from(raw)
                    * i64::from(note.velocity)
                    * i64::from(voice.gain)
                    * i64::from(envelope)
                    / (255 * 255 * 32_767)) as i32;
                voice.phase = voice
                    .phase
                    .wrapping_add(Self::phase_increment(note.pitch, sample_rate));
            }
        }
        voice.cursor = voice.cursor.saturating_add(1);
        output
    }

    fn tone_sample(voice: &mut Voice, tone: AudioTone, sample_rate: u32) -> i32 {
        if voice.delay > 0 {
            voice.delay -= 1;
            return 0;
        }
        let duration = u64::from(tone.duration_ms) * u64::from(sample_rate) / 1_000;
        if duration == 0 || voice.cursor >= duration {
            voice.stop();
            return 0;
        }
        let raw = Self::wave_sample(tone.waveform, voice);
        let envelope = Self::tone_envelope(voice.cursor, duration, sample_rate);
        let output =
            i64::from(raw) * i64::from(tone.gain) * i64::from(envelope) / (255 * 32_767 * 5);
        voice.phase = voice
            .phase
            .wrapping_add(Self::phase_increment(tone.pitch, sample_rate));
        voice.cursor = voice.cursor.saturating_add(1);
        output as i32
    }

    fn tick_sample(tick: u16, ticks_per_second: u16, sample_rate: u32) -> u64 {
        u64::from(tick) * u64::from(sample_rate) / u64::from(ticks_per_second)
    }

    fn phase_increment(pitch: u8, sample_rate: u32) -> u32 {
        const MIDI_ZERO_MILLIHERTZ: [u32; 12] = [
            8_176, 8_662, 9_177, 9_723, 10_301, 10_914, 11_563, 12_250, 12_978, 13_750, 14_568,
            15_434,
        ];
        let octave = pitch / 12;
        let note = usize::from(pitch % 12);
        let frequency = u64::from(MIDI_ZERO_MILLIHERTZ[note]) << octave.min(15);
        ((frequency << 32) / (u64::from(sample_rate) * 1_000)).min(u64::from(u32::MAX)) as u32
    }

    fn wave_sample(waveform: Waveform, voice: &mut Voice) -> i32 {
        let phase = voice.phase;
        match waveform {
            Waveform::Sine => {
                let x = ((phase >> 16) as i32) - 32_768;
                4 * x * (32_768 - x.abs()) / 32_768
            }
            Waveform::Triangle => {
                let value = (phase >> 16) as i32;
                if value < 16_384 {
                    value * 2
                } else if value < 49_152 {
                    65_535 - value * 2
                } else {
                    value * 2 - 131_070
                }
            }
            Waveform::Square => {
                if phase & 0x8000_0000 == 0 {
                    24_000
                } else {
                    -24_000
                }
            }
            Waveform::Noise => {
                let mut noise = voice.noise;
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                voice.noise = noise;
                (noise >> 16) as i16 as i32
            }
        }
    }

    fn envelope(local: u64, duration: u64, sample_rate: u32) -> i32 {
        let attack = u64::from((sample_rate / 200).max(1));
        let release = u64::from((sample_rate / 40).max(1)).min(duration / 2);
        let attack_gain = ((local.min(attack) * 32_767) / attack) as i32;
        let remaining = duration.saturating_sub(local);
        let release_gain = (remaining.min(release) * 32_767)
            .checked_div(release)
            .unwrap_or(32_767) as i32;
        attack_gain.min(release_gain)
    }

    fn tone_envelope(local: u64, duration: u64, sample_rate: u32) -> i32 {
        let attack = u64::from((sample_rate / 125).max(1)).min(duration);
        let attack_gain = ((local.min(attack) * 32_767) / attack.max(1)) as i64;
        let remaining = duration.saturating_sub(local);
        let linear = (remaining * 32_767 / duration.max(1)) as i64;
        let decay = linear * linear / 32_767;
        attack_gain.min(decay) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioCue, NoteEvent, Score, Waveform};

    const TONE: CueId = CueId::new(1);
    const LOOP: CueId = CueId::new(2);
    static TONE_NOTES: [NoteEvent; 1] = [NoteEvent::new(0, 4, 69, 220, Waveform::Sine)];
    static LOOP_NOTES: [NoteEvent; 1] = [NoteEvent::new(0, 1, 57, 180, Waveform::Triangle)];
    static CUES: [AudioCue; 2] = [
        AudioCue::new(TONE, Score::new(100, 5, false, &TONE_NOTES)),
        AudioCue::new(LOOP, Score::new(10, 1, true, &LOOP_NOTES)),
    ];
    static BANK: AudioBank = AudioBank::new(&CUES);

    #[test]
    fn mixer_generates_bounded_nonzero_pcm() {
        let mut mixer = AudioMixer::<4>::new(&BANK, 8_000).unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: TONE,
                gain: 255,
            })
            .unwrap();
        let mut output = [0_i16; 256];
        mixer.render_stereo(&mut output);
        assert!(output.iter().any(|sample| *sample != 0));
        assert!(output.chunks_exact(2).all(|frame| frame[0] == frame[1]));
    }

    #[test]
    fn looping_cue_is_idempotent_and_survives_wrap() {
        let mut mixer = AudioMixer::<2>::new(&BANK, 1_000).unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: LOOP,
                gain: 255,
            })
            .unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: LOOP,
                gain: 255,
            })
            .unwrap();
        assert_eq!(mixer.active_voice_count(), 1);
        let mut output = [0_i16; 400];
        mixer.render_stereo(&mut output);
        assert_eq!(mixer.active_voice_count(), 1);
    }

    #[test]
    fn mute_and_stop_are_immediate() {
        let mut mixer = AudioMixer::<2>::new(&BANK, 8_000).unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: LOOP,
                gain: 255,
            })
            .unwrap();
        mixer.apply(AudioCommand::SetMuted(true)).unwrap();
        let mut output = [1_i16; 64];
        mixer.render_mono(&mut output);
        assert!(output.iter().all(|sample| *sample == 0));
        mixer.apply(AudioCommand::StopAll).unwrap();
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn unknown_cue_is_reported() {
        let mut mixer = AudioMixer::<2>::new(&BANK, 8_000).unwrap();
        assert_eq!(
            mixer.apply(AudioCommand::Play {
                cue: CueId::new(99),
                gain: 255,
            }),
            Err(MixerError::UnknownCue(CueId::new(99)))
        );
    }

    #[test]
    fn direct_tone_uses_fixed_voice_storage() {
        let mut mixer = AudioMixer::<2>::new(&BANK, 8_000).unwrap();
        mixer
            .apply(AudioCommand::Tone(AudioTone::new(
                72,
                Waveform::Triangle,
                40,
                210,
            )))
            .unwrap();
        let mut output = [0_i16; 128];
        mixer.render_mono(&mut output);
        assert!(output.iter().any(|sample| *sample != 0));
        let mut tail = [0_i16; 320];
        mixer.render_mono(&mut tail);
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn direct_tone_delay_defers_pcm_without_allocating_another_voice() {
        let mut mixer = AudioMixer::<1>::new(&BANK, 1_000).unwrap();
        mixer
            .apply(AudioCommand::Tone(
                AudioTone::new(72, Waveform::Sine, 40, 210).with_delay_ms(20),
            ))
            .unwrap();
        let mut silence = [1_i16; 20];
        mixer.render_mono(&mut silence);
        assert!(silence.iter().all(|sample| *sample == 0));
        assert_eq!(mixer.active_voice_count(), 1);
        let mut output = [0_i16; 40];
        mixer.render_mono(&mut output);
        assert!(output.iter().any(|sample| *sample != 0));
        let mut release = [0_i16; 1];
        mixer.render_mono(&mut release);
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn cue_and_direct_tone_occupy_distinct_voices() {
        let mut mixer = AudioMixer::<2>::new(&BANK, 8_000).unwrap();
        mixer
            .apply(AudioCommand::Tone(AudioTone::new(
                72,
                Waveform::Sine,
                100,
                180,
            )))
            .unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: TONE,
                gain: 180,
            })
            .unwrap();
        assert_eq!(mixer.active_voice_count(), 2);
    }
}
