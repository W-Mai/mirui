use super::model::MarbleModel;
use super::types::{MarbleSound, PadTimbre};
use crate::types::Fixed64;

pub(super) const MAX_SOUND_EVENTS: usize = 8;
pub(super) const MAX_RECORDED_NOTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordedNote {
    pub(super) step: u8,
    pub(super) slot: u8,
    pub(super) pitch: u8,
    pub(super) timbre: PadTimbre,
    pub(super) gain: u8,
}

impl MarbleModel {
    fn emit_sound(&mut self, sound: MarbleSound) {
        let index = usize::from(self.sound_len);
        if index < MAX_SOUND_EVENTS {
            self.sounds[index] = Some(sound);
            self.sound_len += 1;
        }
    }

    pub(super) fn emit_pad_sound(&mut self, slot: usize, gain: u8, manual: bool) {
        let Some(mut pad) = self.pads[slot] else {
            return;
        };
        if manual {
            pad.last_hit = self.sim_time;
            self.pads[slot] = Some(pad);
        }
        let current = self.transport_steps;
        let step = if manual {
            self.current_audio_step()
        } else {
            let floor = self.current_audio_step();
            let ceiling = if current > Fixed64::from_int(i64::from(floor)) {
                floor.saturating_add(1)
            } else {
                floor
            };
            ceiling.div_ceil(2) * 2
        };
        let delay_ms = if manual {
            0
        } else {
            ((Fixed64::from_int(i64::from(step)) - current) * Fixed64::from_int(15_000)
                / Fixed64::from_int(i64::from(self.bpm.max(1)))
                + Fixed64::from_ratio(1, 2))
            .to_int()
            .clamp(0, i64::from(u16::MAX)) as u16
        };
        if self.recording && step >= self.record_start_step {
            let relative = step - self.record_start_step;
            let same_step = self.recorded[..usize::from(self.recorded_len)]
                .iter()
                .flatten()
                .filter(|note| u32::from(note.step) == relative)
                .count();
            let index = usize::from(self.recorded_len);
            if relative < 64 && index < MAX_RECORDED_NOTES && same_step < 4 {
                self.recorded[index] = Some(RecordedNote {
                    step: relative as u8,
                    slot: slot as u8,
                    pitch: pad.pitch,
                    timbre: pad.timbre,
                    gain,
                });
                self.recorded_len += 1;
            }
        }
        self.emit_sound(MarbleSound::Pad {
            slot: slot as u8,
            pitch: pad.pitch,
            timbre: pad.timbre,
            gain,
            delay_ms,
        });
    }

    pub(crate) fn transport_beat(&self) -> usize {
        (self.current_audio_step() as usize / 4) % 16
    }

    pub(super) fn current_audio_step(&self) -> u32 {
        self.transport_steps.to_int().max(0) as u32
    }

    pub(super) fn advance_audio(&mut self) -> bool {
        let current = self.current_audio_step();
        let mut model_changed = false;
        if self.recording && current >= self.record_start_step.saturating_add(64) {
            self.finish_recording(false);
            model_changed = true;
        }
        if self.looping && current > self.last_audio_step {
            let first = self
                .last_audio_step
                .saturating_add(1)
                .max(current.saturating_sub(15));
            for step in first..=current {
                if step < self.loop_start_step {
                    continue;
                }
                let local =
                    ((step - self.loop_start_step) % u32::from(self.loop_length_steps)) as u8;
                for index in 0..usize::from(self.recorded_len) {
                    let Some(note) = self.recorded[index] else {
                        continue;
                    };
                    if note.step == local {
                        if let Some(pad) = self.pads[usize::from(note.slot)].as_mut() {
                            pad.pulse = pad.pulse.max(Fixed64::from_ratio(65, 100));
                        }
                        self.emit_sound(MarbleSound::Pad {
                            slot: note.slot,
                            pitch: note.pitch,
                            timbre: note.timbre,
                            gain: (u16::from(note.gain) * 174 / 255) as u8,
                            delay_ms: 0,
                        });
                    }
                }
            }
        }
        self.last_audio_step = current;
        model_changed
    }

    pub(super) fn finish_recording(&mut self, early: bool) {
        let current = self.current_audio_step();
        let elapsed = current.saturating_sub(self.record_start_step).max(1);
        self.recording = false;
        if self.recorded_len == 0 {
            self.stop_recording();
            self.notify("NO NOTES RECORDED / TAP A PAD");
            return;
        }
        self.loop_length_steps = if early {
            elapsed.div_ceil(16).clamp(1, 4) as u8 * 16
        } else {
            64
        };
        self.looping = true;
        self.loop_start_step = current.div_ceil(4) * 4;
        self.last_audio_step = current;
        self.notify("LOOP PLAYING / LIVE NOTES STAY ACTIVE");
    }

    pub(super) fn stop_recording(&mut self) {
        self.recording = false;
        self.looping = false;
        self.recorded = [None; MAX_RECORDED_NOTES];
        self.recorded_len = 0;
        self.loop_length_steps = 16;
    }
}
