use super::{
    Action, Boundary, CommandError, Pattern, Playback, Score, ScoreError, ScoreLimits,
    TICKS_PER_BEAT, Transport,
};
use crate::types::Fixed;
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
    pub chord: &'static str,
    #[observe]
    pub next_chord: &'static str,
    #[observe]
    pub previous_chord: &'static str,
    #[observe]
    pub tempo: String,
    #[observe]
    pub playing: bool,
    #[observe]
    pub clock_active: bool,
    #[observe]
    pub connected: bool,
    #[observe]
    pub bar_phase: u16,
    #[observe]
    pub position_ticks: u64,
    #[observe]
    pub beats: u8,
    #[observe]
    pub beat: u8,
    #[observe]
    pub pattern: Pattern,
    #[observe]
    pub stage_width: crate::types::Fixed,
    #[observe]
    pub large: bool,
    pub(super) sheet: super::sheet::Sheet,
    sheet_bar: u16,
    sheet_view_height: u16,
    sheet_needs_prepare: bool,
    #[observe]
    pub sheet_revision: u32,
    #[observe]
    pub content_height: Fixed,
    #[observe]
    pub next_content_height: Fixed,
    #[observe]
    pub previous_content_height: Fixed,
    #[observe]
    pub previous_scroll: Fixed,
    #[observe]
    pub page_scroll: Fixed,
    #[observe]
    pub next_hint: String,
    #[observe]
    pub has_next: bool,
    #[observe]
    pub sheet_scroll: Fixed,
    #[observe]
    pub bar_label: String,
    #[observe]
    pub slot_index: u8,
}

impl Session {
    pub fn new(score: Score) -> Self {
        let mut session = Self {
            transport: Transport::new(score.milli_bpm, score.meter.beats)
                .expect("validated score timing"),
            title: String::from(score.title()),
            tempo: String::with_capacity(16),
            chord: "",
            next_chord: "",
            previous_chord: "",
            playing: false,
            clock_active: false,
            connected: false,
            bar_phase: 0,
            position_ticks: 0,
            beats: score.meter.beats,
            beat: 0,
            pattern: score.sections()[0].pattern,
            stage_width: crate::types::Fixed::ZERO,
            large: false,
            sheet: super::sheet::Sheet::new(),
            sheet_bar: u16::MAX,
            sheet_view_height: 64,
            sheet_needs_prepare: true,
            sheet_revision: 0,
            content_height: Fixed::ONE,
            next_content_height: Fixed::ONE,
            previous_content_height: Fixed::ONE,
            previous_scroll: Fixed::ZERO,
            page_scroll: Fixed::ZERO,
            next_hint: String::with_capacity(32),
            has_next: false,
            sheet_scroll: Fixed::ZERO,
            bar_label: String::with_capacity(16),
            slot_index: 0,
            score,
            current: usize::MAX,
            now_us: 0,
        };
        session.sync();
        session
    }

    fn sync(&mut self) {
        let position = self.transport.position();
        self.position_ticks = position;
        let bar_ticks = u64::from(self.beats) * TICKS_PER_BEAT;
        let bar = (position / bar_ticks).min(u64::from(self.score.bars() - 1)) as u16;
        self.bar_phase = ((position % bar_ticks) * 256 / bar_ticks) as u16;
        self.beat = ((position % bar_ticks) / TICKS_PER_BEAT) as u8;
        self.playing = self.transport.playback() == Playback::Running;
        self.clock_active = self.playing && self.now_us >= self.transport.starts_at_us();
        self.pattern = self
            .transport
            .rhythm()
            .unwrap_or(self.score.sections()[self.score.section_at(bar)].pattern);
        let next = self.score.chord_at(position);
        if self.sheet_bar != bar {
            self.rebuild_sheet(bar);
        }
        self.slot_index = self.score.chords()[next].slot;
        let slots = u64::from(self.score.chords()[next].slots);
        let local = position % bar_ticks;
        self.has_next = bar + 1 < self.score.bars();
        self.page_scroll = Fixed::from_int(
            self.stage_width.to_int().max(0) / 2
                + (self.stage_width.to_int().max(0) as u64 * local / bar_ticks) as i32,
        );
        let start = (u64::from(self.slot_index) * bar_ticks).div_ceil(slots);
        let end = ((u64::from(self.slot_index) + 1) * bar_ticks).div_ceil(slots);
        self.sheet_scroll = self.sheet.scroll(
            usize::from(self.slot_index),
            local.saturating_sub(start),
            end - start,
            self.sheet_view_height,
        );
        if next != self.current {
            self.current = next;
            let current = &self.score.chords()[next];
            self.chord = self.score.chord_name(current.degree);
            let first = self.score.chords().partition_point(|chord| chord.bar < bar);
            if let Some(previous) = first
                .checked_sub(1)
                .map(|index| &self.score.chords()[index])
            {
                self.previous_chord = self.score.chord_name(previous.degree);
            } else {
                self.previous_chord = "";
            }
            let following = self
                .score
                .chords()
                .partition_point(|chord| chord.bar <= bar);
            if let Some(next) = self.score.chords().get(following) {
                self.next_chord = self.score.chord_name(next.degree);
            } else {
                self.next_chord = "END";
            }
        }
        self.tempo.clear();
        write!(&mut self.tempo, "{} BPM", self.transport.milli_bpm() / 1000)
            .expect("prepared label");
        if self.transport.pending() != 0 {
            self.tempo.push_str(" *");
        }
    }

    fn rebuild_sheet(&mut self, bar: u16) {
        if self.sheet_needs_prepare {
            self.sheet.prepare(
                &self.score,
                self.stage_width.to_int().max(120) as u16,
                self.sheet_view_height,
                self.large,
            );
            self.sheet_needs_prepare = false;
        }
        self.sheet.rebuild(
            &self.score,
            bar,
            self.stage_width.to_int().max(120) as u16,
            self.sheet_view_height,
            self.large,
        );
        self.sheet_bar = bar;
        self.content_height = Fixed::from_int(i32::from(self.sheet.height));
        self.next_content_height = Fixed::from_int(i32::from(self.sheet.next_height));
        self.previous_content_height = Fixed::from_int(i32::from(self.sheet.previous_height));
        self.previous_scroll = (self.previous_content_height
            - Fixed::from_int(i32::from(self.sheet_view_height)))
        .max(Fixed::ZERO);
        self.next_hint.clear();
        if let Some(chord) = self.score.bar_chords(bar + 1).first() {
            self.next_hint.push_str("> ");
            let mut units = (self.stage_width.to_int().max(120) as usize / 6).saturating_sub(11);
            for ch in self.score.lyric(chord).chars() {
                let width = if ch.is_ascii() { 1 } else { 2 };
                if width > units || self.next_hint.len() + ch.len_utf8() > 29 {
                    self.next_hint.push_str("...");
                    break;
                }
                self.next_hint.push(ch);
                units -= width;
            }
        }
        self.sheet_revision = self.sheet_revision.wrapping_add(1);
        self.bar_label.clear();
        write!(
            &mut self.bar_label,
            "{:03}/{:03}",
            bar + 1,
            self.score.bars()
        )
        .expect("bar label");
    }
}

#[crate::model]
impl Session {
    pub fn resize(&mut self, width: crate::types::Fixed, height: crate::types::Fixed) {
        use crate::types::Fixed;
        self.stage_width = (width - Fixed::from_int(8)).max(Fixed::ZERO);
        self.large = width >= Fixed::from_int(256) && height >= Fixed::from_int(200);
        self.sheet_needs_prepare = true;
        self.sheet_bar = u16::MAX;
        self.sync();
    }
    pub fn sheet_viewport(&mut self, width: Fixed, height: Fixed) {
        let height = height.to_int().max(1) as u16;
        if self.stage_width != width || self.sheet_view_height != height {
            self.stage_width = width;
            self.sheet_view_height = height;
            self.sheet_needs_prepare = true;
            self.sheet_bar = u16::MAX;
            self.sync();
        }
    }
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }
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
        let connected = self.connected;
        let stage_width = self.stage_width;
        let large = self.large;
        let sheet_view_height = self.sheet_view_height;
        *self = Self::new(score);
        self.connected = connected;
        self.stage_width = stage_width;
        self.large = large;
        self.sheet_view_height = sheet_view_height;
        self.sheet_needs_prepare = true;
        self.sheet_bar = u16::MAX;
        self.sync();
        Ok(())
    }
}
