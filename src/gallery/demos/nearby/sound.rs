mod bank;

use crate::audio::{AudioBank, CueId};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioCommand, AudioMixer};

    #[test]
    fn every_cue_is_finite_audible_and_unclipped() {
        let bank = audio_bank();
        assert_eq!(bank.cues().len(), Cue::Restored as usize + 1);
        for cue in bank.cues() {
            let mut mixer = AudioMixer::<4>::new(bank, 48_000).unwrap();
            mixer
                .apply(AudioCommand::Play {
                    cue: cue.id,
                    gain: 255,
                })
                .unwrap();
            let mut pcm = alloc::vec![0; 24_000];
            mixer.render_mono(&mut pcm);
            assert!(pcm.iter().any(|sample| *sample != 0));
            assert!(pcm.iter().all(|sample| sample.unsigned_abs() < 30_000));
            assert!(pcm[16_800..].iter().all(|sample| *sample == 0));
            assert_eq!(mixer.active_voice_count(), 0);
        }
    }
}
