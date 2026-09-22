use web_sys::{
    AudioContext, AudioContextState, DynamicsCompressorNode, GainNode, OscillatorNode,
    OscillatorType,
};

use super::{
    AudioBank, AudioCommand, AudioOutputState, AudioSink, AudioTone, CueId, Score, Waveform,
};

const NODE_CAPACITY: usize = 64;
const LOOP_CAPACITY: usize = 4;

struct ScheduledNode {
    cue: Option<CueId>,
    oscillator: OscillatorNode,
    _gain: GainNode,
    ends_at: f64,
}

#[derive(Clone, Copy)]
struct LoopSlot {
    cue: CueId,
    gain: u8,
    next_start: f64,
}

pub struct WebAudioSink {
    bank: Option<&'static AudioBank>,
    context: Option<AudioContext>,
    master: Option<GainNode>,
    compressor: Option<DynamicsCompressorNode>,
    nodes: [Option<ScheduledNode>; NODE_CAPACITY],
    loops: [Option<LoopSlot>; LOOP_CAPACITY],
    master_gain: u8,
    muted: bool,
    state: AudioOutputState,
}

impl Default for WebAudioSink {
    fn default() -> Self {
        Self::new()
    }
}

impl WebAudioSink {
    pub fn new() -> Self {
        Self {
            bank: None,
            context: None,
            master: None,
            compressor: None,
            nodes: core::array::from_fn(|_| None),
            loops: [None; LOOP_CAPACITY],
            master_gain: 220,
            muted: false,
            state: AudioOutputState::Starting,
        }
    }

    fn ensure_context(&mut self) -> Result<(), wasm_bindgen::JsValue> {
        if self.context.is_some() {
            return Ok(());
        }
        let context = AudioContext::new()?;
        let master = context.create_gain()?;
        let compressor = context.create_dynamics_compressor()?;
        compressor.threshold().set_value(-14.0);
        compressor.knee().set_value(16.0);
        compressor.ratio().set_value(6.0);
        compressor.attack().set_value(0.004);
        compressor.release().set_value(0.18);
        master.connect_with_audio_node(&compressor)?;
        compressor.connect_with_audio_node(&context.destination())?;
        self.context = Some(context);
        self.master = Some(master);
        self.compressor = Some(compressor);
        self.apply_master_gain()?;
        Ok(())
    }

    fn observed_state(&self) -> AudioOutputState {
        match self.state {
            AudioOutputState::Locked | AudioOutputState::Ready => {
                if self
                    .context
                    .as_ref()
                    .is_some_and(|context| context.state() == AudioContextState::Running)
                {
                    AudioOutputState::Ready
                } else {
                    AudioOutputState::Locked
                }
            }
            state => state,
        }
    }

    fn refresh_state(&mut self) {
        self.state = self.observed_state();
    }

    fn apply_master_gain(&self) -> Result<(), wasm_bindgen::JsValue> {
        let (Some(context), Some(master)) = (&self.context, &self.master) else {
            return Ok(());
        };
        let gain = if self.muted {
            0.0
        } else {
            f32::from(self.master_gain) / 255.0
        };
        master
            .gain()
            .set_value_at_time(gain, context.current_time())?;
        Ok(())
    }

    fn schedule_cue(
        &mut self,
        cue: CueId,
        gain: u8,
        starts_at: f64,
    ) -> Result<f64, wasm_bindgen::JsValue> {
        let bank = self
            .bank
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("audio bank is not started"))?;
        let score = &bank
            .cue(cue)
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("unknown audio cue"))?
            .score;
        let duration = f64::from(score.length_ticks) / f64::from(score.ticks_per_second.max(1));
        for note in score.notes {
            let start =
                starts_at + f64::from(note.start_ticks) / f64::from(score.ticks_per_second.max(1));
            let end =
                start + f64::from(note.duration_ticks) / f64::from(score.ticks_per_second.max(1));
            self.schedule_note(Some(cue), gain, *note, start, end)?;
        }
        Ok(duration)
    }

    fn schedule_note(
        &mut self,
        cue: Option<CueId>,
        cue_gain: u8,
        note: super::NoteEvent,
        start: f64,
        end: f64,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let Some(slot) = self.nodes.iter().position(Option::is_none) else {
            return Ok(());
        };
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("audio context is locked"))?;
        let master = self
            .master
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("master gain is unavailable"))?;
        let oscillator = context.create_oscillator()?;
        oscillator.set_type(match note.waveform {
            Waveform::Sine => OscillatorType::Sine,
            Waveform::Triangle => OscillatorType::Triangle,
            Waveform::Square => OscillatorType::Square,
            Waveform::Noise => OscillatorType::Sawtooth,
        });
        let frequency = 440.0_f32 * 2.0_f32.powf((f32::from(note.pitch) - 69.0) / 12.0);
        oscillator.frequency().set_value(frequency);
        let gain = context.create_gain()?;
        let peak = (f32::from(note.velocity) / 255.0) * (f32::from(cue_gain) / 255.0) * 0.24;
        let attack_end = (start + 0.008).min(end);
        gain.gain().set_value_at_time(0.0001, start)?;
        gain.gain()
            .exponential_ramp_to_value_at_time(peak.max(0.0001), attack_end)?;
        gain.gain().exponential_ramp_to_value_at_time(0.0001, end)?;
        oscillator.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(master)?;
        oscillator.start_with_when(start)?;
        oscillator.stop_with_when(end + 0.01)?;
        self.nodes[slot] = Some(ScheduledNode {
            cue,
            oscillator,
            _gain: gain,
            ends_at: end + 0.02,
        });
        Ok(())
    }

    fn schedule_tone(&mut self, tone: AudioTone) -> Result<(), wasm_bindgen::JsValue> {
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("audio context is locked"))?;
        let start = context.current_time() + 0.006 + f64::from(tone.delay_ms) / 1_000.0;
        let end = start + f64::from(tone.duration_ms) / 1_000.0;
        self.schedule_note(
            None,
            u8::MAX,
            super::NoteEvent::new(0, 1, tone.pitch, tone.gain, tone.waveform),
            start,
            end,
        )
    }

    fn cleanup(&mut self, now: f64) {
        for node in &mut self.nodes {
            if node.as_ref().is_some_and(|node| node.ends_at <= now) {
                *node = None;
            }
        }
    }

    fn pump_loops(&mut self) -> Result<(), wasm_bindgen::JsValue> {
        let Some(context) = &self.context else {
            return Ok(());
        };
        let now = context.current_time();
        self.cleanup(now);
        for index in 0..LOOP_CAPACITY {
            let Some(mut slot) = self.loops[index] else {
                continue;
            };
            if slot.next_start <= now + 0.35 {
                let duration = self.schedule_cue(slot.cue, slot.gain, slot.next_start)?;
                slot.next_start += duration.max(0.05);
                self.loops[index] = Some(slot);
            }
        }
        Ok(())
    }

    fn stop_cue(&mut self, cue: CueId) {
        for slot in &mut self.loops {
            if slot.is_some_and(|slot| slot.cue == cue) {
                *slot = None;
            }
        }
        for node in &mut self.nodes {
            if node.as_ref().is_some_and(|node| node.cue == Some(cue)) {
                if let Some(active) = node.take() {
                    let _ = active.oscillator.stop();
                }
            }
        }
    }

    fn play(&mut self, cue: CueId, gain: u8) -> Result<(), wasm_bindgen::JsValue> {
        let bank = self
            .bank
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("audio bank is not started"))?;
        let score: &'static Score = &bank
            .cue(cue)
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("unknown audio cue"))?
            .score;
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("audio context is locked"))?;
        let now = context.current_time() + 0.015;
        if score.looping {
            if self.loops.iter().flatten().any(|slot| slot.cue == cue) {
                return Ok(());
            }
            if let Some(slot) = self.loops.iter_mut().find(|slot| slot.is_none()) {
                *slot = Some(LoopSlot {
                    cue,
                    gain,
                    next_start: now,
                });
                self.pump_loops()?;
            }
        } else {
            self.schedule_cue(cue, gain, now)?;
        }
        Ok(())
    }
}

impl AudioSink for WebAudioSink {
    type Error = wasm_bindgen::JsValue;

    fn start(&mut self, bank: &'static AudioBank) -> Result<(), Self::Error> {
        self.bank = Some(bank);
        self.state = AudioOutputState::Locked;
        Ok(())
    }

    fn unlock(&mut self) -> Result<(), Self::Error> {
        self.ensure_context()?;
        if let Some(context) = &self.context {
            let _ = context.resume()?;
        }
        self.state = AudioOutputState::Locked;
        self.refresh_state();
        Ok(())
    }

    fn update(&mut self) -> Result<(), Self::Error> {
        self.refresh_state();
        if self.state == AudioOutputState::Ready {
            self.pump_loops()?;
        }
        Ok(())
    }

    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error> {
        match command {
            AudioCommand::Play { cue, gain } => self.play(cue, gain),
            AudioCommand::Tone(tone) => self.schedule_tone(tone),
            AudioCommand::Stop { cue } => {
                self.stop_cue(cue);
                Ok(())
            }
            AudioCommand::StopAll => {
                for slot in &mut self.loops {
                    *slot = None;
                }
                for node in &mut self.nodes {
                    if let Some(active) = node.take() {
                        let _ = active.oscillator.stop();
                    }
                }
                Ok(())
            }
            AudioCommand::SetMasterGain(gain) => {
                self.master_gain = gain;
                self.apply_master_gain()
            }
            AudioCommand::SetMuted(muted) => {
                self.muted = muted;
                self.apply_master_gain()
            }
        }
    }

    fn suspend(&mut self) {
        if let Some(context) = &self.context {
            let _ = context.suspend();
        }
        self.state = AudioOutputState::Suspended;
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        if let Some(context) = &self.context {
            let _ = context.resume()?;
            self.state = AudioOutputState::Locked;
            self.refresh_state();
        } else {
            self.state = AudioOutputState::Locked;
        }
        Ok(())
    }

    fn stop(&mut self) {
        let _ = self.submit(AudioCommand::StopAll);
        if let Some(context) = self.context.take() {
            let _ = context.close();
        }
        self.master = None;
        self.compressor = None;
        self.state = AudioOutputState::Stopped;
    }

    fn state(&self) -> AudioOutputState {
        self.observed_state()
    }
}
