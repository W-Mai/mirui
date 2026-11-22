use crate::gameplay::Cue;
use mirui::audio::{AudioBank, AudioCue, CueId, NoteEvent, Score, Waveform};

pub(crate) const fn id(cue: Cue) -> CueId {
    CueId::new(cue as u16)
}

macro_rules! tone {
    ($cue:ident, $length:literal, $wave:ident, $first:literal, $last:literal) => {
        AudioCue::new(
            id(Cue::$cue),
            Score::new(
                1000,
                $length,
                false,
                &[
                    NoteEvent::new(0, $length / 2, $first, 62, Waveform::$wave),
                    NoteEvent::new($length / 2, $length / 2, $last, 56, Waveform::$wave),
                ],
            ),
        )
    };
}

static CUES: &[AudioCue] = &[
    tone!(Attach, 70, Triangle, 64, 76),
    tone!(Release, 64, Triangle, 78, 58),
    tone!(Hit, 98, Noise, 38, 22),
    tone!(Warning, 150, Square, 67, 67),
    tone!(Collect, 152, Triangle, 79, 91),
    tone!(Revive, 176, Triangle, 56, 68),
    tone!(Depart, 240, Triangle, 79, 88),
];
static BANK: AudioBank = AudioBank::new(CUES);

pub fn audio_bank() -> &'static AudioBank {
    &BANK
}
