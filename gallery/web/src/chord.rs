use gallery::mirui::gallery::demos::chord_prompter::{
    Action, Boundary, Pattern, Playback, Score, ScoreLimits, Stroke, TICKS_PER_BEAT, Transport,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn chord_center_emphasis(distance: i32) -> u16 {
    gallery::mirui::gallery::demos::chord_prompter::center_emphasis(distance)
}

fn micros(millis: f64) -> u64 {
    (millis.max(0.0) * 1000.0) as u64
}

/// Browser preview using the same bounded score parser and musical transport as the display.
#[wasm_bindgen]
pub struct ChordPreview {
    score: Score,
    transport: Transport,
}

#[wasm_bindgen]
impl ChordPreview {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<ChordPreview, JsValue> {
        let score = Score::parse(source, ScoreLimits::DEVICE)
            .map_err(|error| JsValue::from_str(&alloc::format!("{error:?}")))?;
        let transport =
            Transport::new(score.milli_bpm, score.meter.beats).expect("validated score timing");
        Ok(Self { score, transport })
    }

    pub fn score_json(&self) -> String {
        let bar_ticks = u64::from(self.score.meter.beats) * TICKS_PER_BEAT;
        let mut out = alloc::format!(
            "{{\"title\":\"{}\",\"bpm\":{},\"beats\":{},\"unit\":{},\"bars\":{},\"slots\":[",
            super::json_escape(self.score.title()),
            self.score.milli_bpm,
            self.score.meter.beats,
            self.score.meter.unit,
            self.score.bars(),
        );
        for (index, chord) in self.score.chords().iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let start = u64::from(chord.bar) * bar_ticks
                + (u64::from(chord.slot) * bar_ticks).div_ceil(u64::from(chord.slots));
            let section = &self.score.sections()[self.score.section_at(chord.bar)];
            out.push_str(&alloc::format!(
                "{{\"tick\":{},\"chord\":\"{}\",\"lyric\":\"{}\",\"section\":\"{}\"}}",
                start,
                self.score.chord_name(chord.degree),
                super::json_escape(&self.score.lyric(chord).to_string()),
                super::json_escape(self.score.text(section.name.clone())),
            ));
        }
        out.push_str("],\"sections\":[");
        for (index, section) in self.score.sections().iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            out.push_str(&alloc::format!(
                "{{\"tick\":{},\"name\":\"{}\"}}",
                u64::from(section.first_bar) * bar_ticks,
                super::json_escape(self.score.text(section.name.clone())),
            ));
        }
        out.push_str("]}");
        out
    }

    pub fn tick(&mut self, now_ms: f64) -> u32 {
        let now = micros(now_ms);
        let end = u64::from(self.score.bars()) * u64::from(self.score.meter.beats) * TICKS_PER_BEAT;
        if self.transport.advance(now) >= end {
            self.transport.pause(now);
            let _ = self
                .transport
                .schedule(now, Boundary::Now, Action::Seek(end - 1));
        }
        self.transport.position() as u32
    }

    /// Starts or resumes; a future time starts after a count-in.
    pub fn play(&mut self, now_ms: f64) {
        let end = u64::from(self.score.bars()) * u64::from(self.score.meter.beats) * TICKS_PER_BEAT;
        if self.transport.position() >= end - 1 {
            self.transport.stop();
        }
        self.transport.start(micros(now_ms));
    }

    pub fn pause(&mut self, now_ms: f64) {
        self.transport.pause(micros(now_ms));
    }
    pub fn stop(&mut self) {
        self.transport.stop();
    }

    pub fn tempo(&mut self, now_ms: f64, milli_bpm: u32) -> Result<(), JsValue> {
        self.schedule(now_ms, Action::Tempo(milli_bpm), Boundary::Beat)
    }

    /// Jumps at the next bar line while playing, and at once otherwise.
    pub fn seek(&mut self, now_ms: f64, bar: u16) -> Result<(), JsValue> {
        if bar >= self.score.bars() {
            return Err(JsValue::from_str("Bar is outside this song"));
        }
        self.schedule(
            now_ms,
            Action::Seek(u64::from(bar) * u64::from(self.score.meter.beats) * TICKS_PER_BEAT),
            Boundary::Bar,
        )
    }

    /// Drops a queued jump; playback continues into the following bar.
    pub fn cancel_jump(&mut self) {
        self.transport.cancel_seek();
    }

    /// Continues `from` in this song after an edit: the same bar and offset,
    /// playback state and strumming override, and its live tempo unless this
    /// song sets a different one.
    pub fn adopt(&mut self, now_ms: f64, from: &mut ChordPreview) {
        let now = micros(now_ms);
        let old = u64::from(from.tick(now_ms));
        let old_bar = u64::from(from.score.meter.beats) * TICKS_PER_BEAT;
        let bar_ticks = u64::from(self.score.meter.beats) * TICKS_PER_BEAT;
        let bar = (old / old_bar).min(u64::from(self.score.bars() - 1));
        let tick = bar * bar_ticks + (old % old_bar).min(bar_ticks - 1);
        let _ = self
            .transport
            .schedule(now, Boundary::Now, Action::Seek(tick));
        if self.score.milli_bpm == from.score.milli_bpm {
            let _ = self.transport.schedule(
                now,
                Boundary::Now,
                Action::Tempo(from.transport.milli_bpm()),
            );
        }
        if let Some(pattern) = from.transport.rhythm() {
            let _ = self
                .transport
                .schedule(now, Boundary::Now, Action::Rhythm(Some(pattern)));
        }
        match from.transport.playback() {
            Playback::Running => self.transport.start(now.max(from.transport.starts_at_us())),
            Playback::Paused => self.transport.pause(now),
            Playback::Stopped => {}
        }
    }

    pub fn rhythm(&mut self, now_ms: f64, index: u8) -> Result<(), JsValue> {
        let pattern = if index == 255 {
            None
        } else {
            Some(
                *Pattern::ALL
                    .get(usize::from(index))
                    .ok_or_else(|| JsValue::from_str("Unknown rhythm"))?,
            )
        };
        self.schedule(now_ms, Action::Rhythm(pattern), Boundary::Bar)
    }

    #[wasm_bindgen(getter)]
    pub fn playing(&self) -> bool {
        self.transport.playback() == Playback::Running
    }
    #[wasm_bindgen(getter)]
    pub fn bpm(&self) -> u32 {
        self.transport.milli_bpm()
    }
    #[wasm_bindgen(getter)]
    pub fn pending(&self) -> bool {
        self.transport.pending() != 0
    }
    /// The tick of a queued jump, if any.
    #[wasm_bindgen(getter)]
    pub fn jump_at(&self) -> Option<u32> {
        self.transport.pending_seek().map(|(at, _)| at as u32)
    }
    /// Where a queued jump lands, if any.
    #[wasm_bindgen(getter)]
    pub fn jump_to(&self) -> Option<u32> {
        self.transport.pending_seek().map(|(_, to)| to as u32)
    }
    #[wasm_bindgen(getter)]
    pub fn pattern(&self) -> u8 {
        let bar = (self.transport.position() / (u64::from(self.score.meter.beats) * TICKS_PER_BEAT))
            .min(u64::from(self.score.bars() - 1)) as u16;
        let pattern = self
            .transport
            .rhythm()
            .unwrap_or(self.score.sections()[self.score.section_at(bar)].pattern);
        Pattern::ALL
            .iter()
            .position(|value| *value == pattern)
            .unwrap_or(0) as u8
    }
}

impl ChordPreview {
    fn schedule(&mut self, now_ms: f64, action: Action, boundary: Boundary) -> Result<(), JsValue> {
        self.transport
            .schedule(micros(now_ms), boundary, action)
            .map(|_| ())
            .map_err(|error| JsValue::from_str(&alloc::format!("{error:?}")))
    }
}

#[wasm_bindgen]
pub fn chord_patterns_json() -> String {
    let mut out = String::from("[");
    for (index, pattern) in Pattern::ALL.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push('[');
        for (index, attack) in pattern.attacks().iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let stroke = match attack.stroke {
                Stroke::Down => "down",
                Stroke::Up => "up",
                Stroke::Pick => "pick",
            };
            out.push_str(&alloc::format!(
                "{{\"offset\":{},\"stroke\":\"{}\",\"low\":{},\"high\":{}}}",
                attack.offset,
                stroke,
                attack.strings[0],
                attack.strings[1]
            ));
        }
        out.push(']');
    }
    out.push(']');
    out
}
