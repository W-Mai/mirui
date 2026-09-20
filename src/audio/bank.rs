#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CueId(u16);

impl CueId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Waveform {
    Sine,
    Triangle,
    Square,
    Noise,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTone {
    pub pitch: u8,
    pub waveform: Waveform,
    pub duration_ms: u16,
    pub gain: u8,
    pub delay_ms: u16,
}

impl AudioTone {
    pub const fn new(pitch: u8, waveform: Waveform, duration_ms: u16, gain: u8) -> Self {
        Self {
            pitch,
            waveform,
            duration_ms,
            gain,
            delay_ms: 0,
        }
    }

    pub const fn with_delay_ms(mut self, delay_ms: u16) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteEvent {
    pub start_ticks: u16,
    pub duration_ticks: u16,
    pub pitch: u8,
    pub velocity: u8,
    pub waveform: Waveform,
}

impl NoteEvent {
    pub const fn new(
        start_ticks: u16,
        duration_ticks: u16,
        pitch: u8,
        velocity: u8,
        waveform: Waveform,
    ) -> Self {
        Self {
            start_ticks,
            duration_ticks,
            pitch,
            velocity,
            waveform,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Score {
    pub ticks_per_second: u16,
    pub length_ticks: u16,
    pub looping: bool,
    pub notes: &'static [NoteEvent],
}

impl Score {
    pub const fn new(
        ticks_per_second: u16,
        length_ticks: u16,
        looping: bool,
        notes: &'static [NoteEvent],
    ) -> Self {
        Self {
            ticks_per_second,
            length_ticks,
            looping,
            notes,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioCue {
    pub id: CueId,
    pub score: Score,
}

impl AudioCue {
    pub const fn new(id: CueId, score: Score) -> Self {
        Self { id, score }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioBank {
    cues: &'static [AudioCue],
}

impl AudioBank {
    pub const fn new(cues: &'static [AudioCue]) -> Self {
        Self { cues }
    }

    pub const fn cues(&self) -> &'static [AudioCue] {
        self.cues
    }

    pub fn cue(&'static self, id: CueId) -> Option<&'static AudioCue> {
        self.cues.iter().find(|cue| cue.id == id)
    }
}
