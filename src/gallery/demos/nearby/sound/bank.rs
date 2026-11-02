use super::Cue;
use crate::audio::{AudioBank, AudioCue, NoteEvent, Score, Waveform};

static CUES: &[AudioCue] = &[
    AudioCue::new(
        Cue::Focus.id(),
        Score::new(
            1000,
            40,
            false,
            &[
                NoteEvent::new(0, 20, 84, 58, Waveform::Square),
                NoteEvent::new(20, 19, 72, 58, Waveform::Square),
                NoteEvent::new(0, 12, 60, 22, Waveform::Noise),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Confirm.id(),
        Score::new(
            1000,
            116,
            false,
            &[
                NoteEvent::new(0, 28, 72, 58, Waveform::Square),
                NoteEvent::new(30, 28, 79, 58, Waveform::Square),
                NoteEvent::new(60, 55, 84, 58, Waveform::Square),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Back.id(),
        Score::new(
            1000,
            105,
            false,
            &[
                NoteEvent::new(0, 30, 79, 58, Waveform::Square),
                NoteEvent::new(32, 30, 72, 58, Waveform::Square),
                NoteEvent::new(64, 40, 60, 58, Waveform::Square),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Refresh.id(),
        Score::new(
            1000,
            111,
            false,
            &[
                NoteEvent::new(0, 22, 72, 58, Waveform::Square),
                NoteEvent::new(25, 22, 84, 58, Waveform::Square),
                NoteEvent::new(50, 22, 76, 58, Waveform::Square),
                NoteEvent::new(75, 35, 88, 58, Waveform::Square),
                NoteEvent::new(0, 12, 60, 22, Waveform::Noise),
            ],
        ),
    ),
    AudioCue::new(
        Cue::RoomFound.id(),
        Score::new(
            1000,
            176,
            false,
            &[
                NoteEvent::new(0, 32, 79, 58, Waveform::Square),
                NoteEvent::new(35, 35, 91, 58, Waveform::Square),
                NoteEvent::new(75, 32, 83, 58, Waveform::Square),
                NoteEvent::new(110, 65, 95, 58, Waveform::Square),
            ],
        ),
    ),
    AudioCue::new(
        Cue::JoinRequest.id(),
        Score::new(
            1000,
            246,
            false,
            &[
                NoteEvent::new(0, 35, 67, 58, Waveform::Square),
                NoteEvent::new(40, 40, 79, 58, Waveform::Square),
                NoteEvent::new(140, 35, 67, 58, Waveform::Square),
                NoteEvent::new(180, 65, 86, 58, Waveform::Square),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Ready.id(),
        Score::new(
            1000,
            256,
            false,
            &[
                NoteEvent::new(0, 32, 72, 58, Waveform::Square),
                NoteEvent::new(35, 32, 76, 58, Waveform::Square),
                NoteEvent::new(70, 32, 79, 58, Waveform::Square),
                NoteEvent::new(105, 32, 84, 58, Waveform::Square),
                NoteEvent::new(140, 32, 79, 58, Waveform::Square),
                NoteEvent::new(175, 80, 88, 58, Waveform::Square),
                NoteEvent::new(0, 95, 48, 35, Waveform::Triangle),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Start.id(),
        Score::new(
            1000,
            276,
            false,
            &[
                NoteEvent::new(0, 22, 60, 58, Waveform::Square),
                NoteEvent::new(25, 22, 72, 58, Waveform::Square),
                NoteEvent::new(50, 22, 76, 58, Waveform::Square),
                NoteEvent::new(75, 22, 79, 58, Waveform::Square),
                NoteEvent::new(100, 22, 84, 58, Waveform::Square),
                NoteEvent::new(125, 22, 88, 58, Waveform::Square),
                NoteEvent::new(150, 28, 91, 58, Waveform::Square),
                NoteEvent::new(180, 95, 96, 58, Waveform::Square),
                NoteEvent::new(0, 12, 60, 22, Waveform::Noise),
                NoteEvent::new(0, 95, 48, 35, Waveform::Triangle),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Error.id(),
        Score::new(
            1000,
            176,
            false,
            &[
                NoteEvent::new(0, 25, 60, 58, Waveform::Square),
                NoteEvent::new(27, 25, 59, 58, Waveform::Square),
                NoteEvent::new(54, 35, 55, 58, Waveform::Square),
                NoteEvent::new(105, 70, 48, 58, Waveform::Square),
                NoteEvent::new(0, 12, 60, 22, Waveform::Noise),
            ],
        ),
    ),
    AudioCue::new(
        Cue::Restored.id(),
        Score::new(
            1000,
            181,
            false,
            &[
                NoteEvent::new(0, 30, 84, 58, Waveform::Square),
                NoteEvent::new(34, 30, 79, 58, Waveform::Square),
                NoteEvent::new(68, 30, 76, 58, Waveform::Square),
                NoteEvent::new(105, 75, 72, 58, Waveform::Square),
                NoteEvent::new(0, 95, 48, 35, Waveform::Triangle),
            ],
        ),
    ),
];
pub(super) static BANK: AudioBank = AudioBank::new(CUES);
