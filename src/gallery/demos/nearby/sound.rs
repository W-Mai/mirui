mod bank;

use crate::audio::{AudioBank, AudioHandle, AudioOutputState, AudioStateSignal, CueId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(super) enum Cue {
    Focus,
    Confirm,
    Back,
    Refresh,
    RoomFound,
    JoinRequest,
    Ready,
    Start,
    Error,
    Restored,
}

impl Cue {
    const fn id(self) -> CueId {
        CueId::new(self as u16)
    }
}

pub(super) fn audio_bank() -> &'static AudioBank {
    &bank::BANK
}

#[derive(Clone, Copy, Default)]
pub(super) struct Effect {
    cue: Option<Cue>,
    preferences: Option<(bool, u8)>,
}

#[derive(Default)]
pub(super) struct Feedback {
    elapsed_ms: u64,
    last_focus: Option<u64>,
    last_action: Option<u64>,
    pending: Effect,
    state: Option<AudioStateSignal>,
}

impl Feedback {
    pub fn bind(&mut self, state: Option<AudioStateSignal>) {
        self.state = state;
    }

    pub fn advance(&mut self, delta: u16) {
        self.elapsed_ms = self.elapsed_ms.saturating_add(u64::from(delta));
    }

    pub fn emit(&mut self, cue: Cue) {
        let priority = |cue| match cue {
            Cue::Focus => 0,
            Cue::Confirm | Cue::Back | Cue::Refresh => 1,
            _ => 2,
        };
        if self
            .pending
            .cue
            .is_none_or(|old| priority(cue) >= priority(old))
        {
            self.pending.cue = Some(cue);
        }
    }

    pub fn preferences(&self, sound: bool, volume: u8) -> (bool, u8) {
        self.state.as_ref().map_or((sound, volume), |signal| {
            let state = signal.get();
            (
                !state.muted,
                ((u16::from(state.master_gain) * 100 + 127) / 255) as u8,
            )
        })
    }

    pub fn configure(&mut self, sound: bool, volume: u8) {
        self.pending.preferences = Some((sound, gain(volume)));
        self.emit(Cue::Confirm);
    }

    pub fn take(&mut self) -> Option<Effect> {
        let mut effect = core::mem::take(&mut self.pending);
        if let Some(cue) = effect.cue {
            let limit = match cue {
                Cue::Focus => Some((&mut self.last_focus, 65)),
                Cue::Confirm | Cue::Back | Cue::Refresh => Some((&mut self.last_action, 120)),
                _ => None,
            };
            if let Some((last, interval)) = limit {
                if last.is_some_and(|at| self.elapsed_ms.saturating_sub(at) < interval) {
                    effect.cue = None;
                } else {
                    *last = Some(self.elapsed_ms);
                }
            }
        }
        (effect.cue.is_some() || effect.preferences.is_some()).then_some(effect)
    }
}

pub(super) fn gain(volume: u8) -> u8 {
    ((u16::from(volume.min(100)) * 255 + 50) / 100) as u8
}

pub(super) fn submit(audio: &AudioHandle, effect: &Effect) {
    if let Some((sound, gain)) = effect.preferences {
        if let Some(state) = audio.state() {
            if state.muted == sound {
                audio.set_muted(!sound);
            }
            if state.master_gain != gain {
                audio.set_master_gain(gain);
            }
        }
    }
    let Some(cue) = effect.cue else { return };
    if !audio
        .state()
        .is_some_and(|state| state.output == AudioOutputState::Ready && !state.muted)
    {
        return;
    }
    audio.stop(Cue::Focus.id());
    if cue != Cue::Focus {
        audio.stop(cue.id());
    }
    audio.play(cue.id());
}

#[cfg(test)]
mod tests;
