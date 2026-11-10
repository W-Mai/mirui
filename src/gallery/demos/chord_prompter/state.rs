use super::{
    Action, Boundary, CommandError, Pattern, Playback, Score, ScoreError, ScoreLimits,
    TICKS_PER_BEAT, Transport,
};
use alloc::string::String;
use core::fmt::Write;

#[crate::model]
pub struct Session {
    score: Score,
    transport: Transport,
    current: usize,
    now_us: u64,
    #[observe]
    pub title: String,
    #[observe]
    pub lyric: String,
    #[observe]
    pub next_lyric: String,
    #[observe]
    pub chord: &'static str,
    #[observe]
    pub next_chord: &'static str,
    #[observe]
    pub tempo: String,
    #[observe]
    pub progress: String,
    #[observe]
    pub playing: bool,
    #[observe]
    pub bar_phase: u16,
    #[observe]
    pub beats: u8,
    #[observe]
    pub beat: u8,
    #[observe]
    pub pattern: Pattern,
}

impl Session {
    pub fn new(score: Score) -> Self {
        let capacity = score
            .chords()
            .iter()
            .map(|chord| chord.lyric.len())
            .max()
            .unwrap_or(0);
        let mut session = Self {
            transport: Transport::new(score.milli_bpm, score.meter.beats)
                .expect("validated score timing"),
            title: String::from(score.title()),
            lyric: String::with_capacity(capacity),
            next_lyric: String::with_capacity(capacity),
            tempo: String::with_capacity(16),
            progress: String::with_capacity(24),
            chord: "",
            next_chord: "",
            playing: false,
            bar_phase: 0,
            beats: score.meter.beats,
            beat: 0,
            pattern: score.sections()[0].pattern,
            score,
            current: usize::MAX,
            now_us: 0,
        };
        session.sync();
        session
    }

    fn sync(&mut self) {
        let position = self.transport.position();
        let bar_ticks = u64::from(self.beats) * TICKS_PER_BEAT;
        let bar = (position / bar_ticks).min(u64::from(self.score.bars() - 1)) as u16;
        self.bar_phase = ((position % bar_ticks) * 256 / bar_ticks) as u16;
        self.beat = ((position % bar_ticks) / TICKS_PER_BEAT) as u8;
        self.playing = self.transport.playback() == Playback::Running;
        self.pattern = self
            .transport
            .rhythm()
            .unwrap_or(self.score.sections()[self.score.section_at(bar)].pattern);
        let next = self.score.chord_at(position);
        if next != self.current {
            self.current = next;
            let current = &self.score.chords()[next];
            self.lyric.clear();
            self.lyric.push_str(self.score.text(current.lyric.clone()));
            self.chord = self.score.chord_name(current.degree);
            self.next_lyric.clear();
            if let Some(next) = self.score.chords().get(next + 1) {
                self.next_lyric
                    .push_str(self.score.text(next.lyric.clone()));
                self.next_chord = self.score.chord_name(next.degree);
            } else {
                self.next_chord = "END";
            }
        }
        self.progress.clear();
        write!(
            &mut self.progress,
            "BAR {:02}/{:02}",
            bar + 1,
            self.score.bars()
        )
        .expect("prepared label");
        self.tempo.clear();
        write!(&mut self.tempo, "{} BPM", self.transport.milli_bpm() / 1000)
            .expect("prepared label");
        if self.transport.pending() != 0 {
            self.tempo.push_str(" *");
        }
    }
}

#[crate::model]
impl Session {
    pub fn position(&self) -> u64 {
        self.transport.position()
    }
    pub fn milli_bpm(&self) -> u32 {
        self.transport.milli_bpm()
    }
    pub fn playback(&self) -> Playback {
        self.transport.playback()
    }
    pub fn starts_at_us(&self) -> u64 {
        self.transport.starts_at_us()
    }
    pub fn bars(&self) -> u16 {
        self.score.bars()
    }
    pub fn unit(&self) -> u8 {
        self.score.meter.unit
    }
    pub fn pending(&self) -> usize {
        self.transport.pending()
    }

    #[model(local)]
    pub(super) fn play_at(&mut self, now_us: u64, delay_ms: u16) -> u64 {
        if !self.playing {
            let end = u64::from(self.score.bars()) * u64::from(self.beats) * TICKS_PER_BEAT;
            if self.position() >= end - 1 {
                self.transport.stop();
            }
            self.transport
                .start(now_us.saturating_add(u64::from(delay_ms) * 1000));
        }
        self.now_us = now_us;
        self.sync();
        self.transport.starts_at_us()
    }

    pub fn pause(&mut self, now_us: u64) {
        self.transport.pause(now_us);
        self.now_us = now_us;
        self.sync();
    }

    #[model(local)]
    pub(super) fn control(
        &mut self,
        now_us: u64,
        action: Action,
        boundary: Boundary,
        target: Option<u64>,
    ) -> Result<u64, CommandError> {
        self.now_us = now_us;
        let target = if let Some(tick) = target {
            self.transport.schedule_at(now_us, tick, action)?
        } else {
            self.transport.schedule(now_us, boundary, action)?
        };
        self.sync();
        Ok(target)
    }
    pub fn advance(&mut self, now_us: u64) {
        self.now_us = now_us;
        self.transport.advance(now_us);
        let end = u64::from(self.score.bars()) * u64::from(self.beats) * TICKS_PER_BEAT;
        if self.transport.position() >= end && self.transport.playback() == Playback::Running {
            self.transport.pause(now_us);
            self.transport
                .schedule(now_us, Boundary::Now, Action::Seek(end - 1))
                .expect("immediate seek");
        }
        self.sync();
    }
    pub fn toggle(&mut self, now_us: u64) {
        if self.transport.playback() == Playback::Running {
            self.transport.pause(now_us);
        } else {
            self.play_at(now_us, 0);
        }
        self.now_us = now_us;
        self.sync();
    }
    pub fn stop(&mut self) {
        self.transport.stop();
        self.sync();
    }
    pub fn adjust_tempo(&mut self, delta: i32) -> Result<(), CommandError> {
        let tempo = (i64::from(self.transport.milli_bpm()) + i64::from(delta) * 1000)
            .clamp(20_000, 300_000) as u32;
        self.transport
            .schedule(self.now_us, Boundary::Beat, Action::Tempo(tempo))?;
        self.sync();
        Ok(())
    }
    pub fn next_section(&mut self) -> Result<(), CommandError> {
        let bar_ticks = u64::from(self.beats) * TICKS_PER_BEAT;
        let current =
            (self.transport.position() / bar_ticks).min(u64::from(self.score.bars() - 1)) as u16;
        let next = (self.score.section_at(current) + 1) % self.score.sections().len();
        let bar = u64::from(self.score.sections()[next].first_bar);
        self.transport
            .schedule(self.now_us, Boundary::Bar, Action::Seek(bar * bar_ticks))?;
        self.sync();
        Ok(())
    }
    pub fn load(&mut self, source: &str) -> Result<(), ScoreError> {
        let score = Score::parse(source, ScoreLimits::DEVICE)?;
        super::font::validate(&score)?;
        *self = Self::new(score);
        Ok(())
    }
}
