use alloc::{string::String, vec::Vec};
use core::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Meter {
    pub beats: u8,
    pub unit: u8,
}

/// Strumming events are positioned within a whole bar, independently of chords.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pattern {
    Whole,
    Quarter,
    Pop,
    PopRock,
    Ballad,
    Folk,
    Rock,
    Punk,
    Reggae,
    Shuffle,
    Country,
    Arpeggio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stroke {
    Down,
    Up,
    Pick,
}

#[derive(Clone, Copy, Debug)]
pub struct Attack {
    /// Position in hundredths of a four-beat reference bar.
    pub offset: u16,
    pub stroke: Stroke,
    pub velocity: u8,
    /// Inclusive string indices: zero is low E, five is high E.
    pub strings: [u8; 2],
}

const fn hit(offset: u16, stroke: Stroke, velocity: u8, lo: u8, hi: u8) -> Attack {
    Attack {
        offset,
        stroke,
        velocity,
        strings: [lo, hi],
    }
}

impl Pattern {
    pub const ALL: [Self; 12] = [
        Self::Whole,
        Self::Quarter,
        Self::Pop,
        Self::PopRock,
        Self::Ballad,
        Self::Folk,
        Self::Rock,
        Self::Punk,
        Self::Reggae,
        Self::Shuffle,
        Self::Country,
        Self::Arpeggio,
    ];
    pub const fn attacks(self) -> &'static [Attack] {
        use Stroke::{Down as D, Pick as P, Up as U};
        match self {
            Self::Whole => const { &[hit(0, D, 100, 0, 5)] },
            Self::Quarter => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(100, D, 70, 0, 5),
                        hit(200, D, 80, 0, 5),
                        hit(300, D, 70, 0, 5),
                    ]
                }
            }
            Self::Pop => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(100, D, 70, 0, 5),
                        hit(150, U, 50, 2, 5),
                        hit(250, U, 50, 2, 5),
                        hit(300, D, 70, 0, 5),
                        hit(350, U, 50, 2, 5),
                    ]
                }
            }
            Self::PopRock => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(100, D, 70, 0, 5),
                        hit(150, U, 50, 2, 5),
                        hit(200, D, 80, 0, 5),
                        hit(250, U, 50, 2, 5),
                        hit(300, D, 70, 0, 5),
                        hit(350, U, 50, 2, 5),
                    ]
                }
            }
            Self::Ballad => {
                const {
                    &[
                        hit(0, D, 100, 0, 2),
                        hit(100, U, 40, 3, 5),
                        hit(150, U, 30, 3, 5),
                        hit(200, D, 60, 0, 2),
                        hit(250, U, 40, 3, 5),
                        hit(300, D, 50, 2, 5),
                    ]
                }
            }
            Self::Folk => {
                const {
                    &[
                        hit(0, D, 100, 0, 2),
                        hit(50, U, 30, 3, 5),
                        hit(100, D, 60, 3, 5),
                        hit(150, U, 40, 3, 5),
                        hit(200, D, 80, 0, 2),
                        hit(250, U, 30, 3, 5),
                        hit(300, D, 60, 3, 5),
                        hit(350, U, 40, 3, 5),
                    ]
                }
            }
            Self::Rock => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(100, D, 90, 0, 5),
                        hit(150, U, 50, 2, 5),
                        hit(250, U, 50, 2, 5),
                        hit(300, D, 90, 0, 5),
                    ]
                }
            }
            Self::Punk => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(50, U, 80, 0, 5),
                        hit(100, D, 90, 0, 5),
                        hit(150, U, 80, 0, 5),
                        hit(200, D, 100, 0, 5),
                        hit(250, U, 80, 0, 5),
                        hit(300, D, 90, 0, 5),
                        hit(350, U, 80, 0, 5),
                    ]
                }
            }
            Self::Reggae => {
                const {
                    &[
                        hit(50, U, 80, 2, 5),
                        hit(150, U, 80, 2, 5),
                        hit(250, U, 80, 2, 5),
                        hit(350, U, 80, 2, 5),
                    ]
                }
            }
            Self::Shuffle => {
                const {
                    &[
                        hit(0, D, 100, 0, 5),
                        hit(67, U, 50, 2, 5),
                        hit(100, D, 70, 0, 5),
                        hit(167, U, 50, 2, 5),
                        hit(200, D, 80, 0, 5),
                        hit(267, U, 50, 2, 5),
                        hit(300, D, 70, 0, 5),
                        hit(367, U, 50, 2, 5),
                    ]
                }
            }
            Self::Country => {
                const {
                    &[
                        hit(0, D, 100, 0, 2),
                        hit(100, D, 70, 2, 5),
                        hit(150, U, 40, 3, 5),
                        hit(200, D, 80, 0, 2),
                        hit(250, U, 40, 3, 5),
                        hit(300, D, 70, 2, 5),
                        hit(350, U, 40, 3, 5),
                    ]
                }
            }
            Self::Arpeggio => {
                const {
                    &[
                        hit(0, P, 100, 1, 1),
                        hit(50, P, 100, 3, 3),
                        hit(100, P, 100, 4, 4),
                        hit(150, P, 100, 3, 3),
                        hit(200, P, 100, 5, 5),
                        hit(250, P, 100, 3, 3),
                        hit(300, P, 100, 4, 4),
                        hit(350, P, 100, 3, 3),
                    ]
                }
            }
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "whole" => Self::Whole,
            "quarter" => Self::Quarter,
            "pop" => Self::Pop,
            "poprock" => Self::PopRock,
            "ballad" => Self::Ballad,
            "folk" => Self::Folk,
            "rock" => Self::Rock,
            "punk" => Self::Punk,
            "reggae" => Self::Reggae,
            "shuffle" => Self::Shuffle,
            "country" => Self::Country,
            "arpeggio" => Self::Arpeggio,
            _ => return None,
        })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Quarter => "quarter",
            Self::Pop => "pop",
            Self::PopRock => "poprock",
            Self::Ballad => "ballad",
            Self::Folk => "folk",
            Self::Rock => "rock",
            Self::Punk => "punk",
            Self::Reggae => "reggae",
            Self::Shuffle => "shuffle",
            Self::Country => "country",
            Self::Arpeggio => "arpeggio",
        }
    }
}

/// Bounds apply before retaining source data or growing the parsed tables.
#[derive(Clone, Copy, Debug)]
pub struct ScoreLimits {
    pub bytes: usize,
    pub lyric_bytes: usize,
    pub label_bytes: usize,
    pub bars: usize,
    pub chords: usize,
    pub sections: usize,
}

impl ScoreLimits {
    pub const DEVICE: Self = Self {
        bytes: 8192,
        lyric_bytes: 128,
        label_bytes: 64,
        bars: 128,
        chords: 256,
        sections: 16,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreError {
    Capacity,
    Header { line: usize },
    Tempo { line: usize },
    Meter { line: usize },
    Pattern { line: usize },
    Bars { line: usize },
    Degree { line: usize },
    Accent { line: usize },
    Empty,
}

#[derive(Clone, Debug)]
pub struct Chord {
    pub degree: u8,
    pub lyric: Range<usize>,
    pub bar: u16,
    pub slot: u8,
    pub slots: u8,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: Range<usize>,
    pub first_bar: u16,
    pub pattern: Pattern,
}

#[derive(Debug)]
pub struct Score {
    text: String,
    title: Range<usize>,
    pub key: u8,
    pub milli_bpm: u32,
    pub meter: Meter,
    sections: Vec<Section>,
    chords: Vec<Chord>,
    bars: u16,
}

impl Score {
    /// Parses the song-sheet subset, retaining all text in one contiguous arena.
    pub fn parse(source: &str, limits: ScoreLimits) -> Result<Self, ScoreError> {
        if source.len() > limits.bytes {
            return Err(ScoreError::Capacity);
        }
        let mut score = Self {
            text: String::with_capacity(source.len()),
            title: 0..0,
            key: 0,
            milli_bpm: 96_000,
            meter: Meter { beats: 4, unit: 4 },
            sections: Vec::new(),
            chords: Vec::new(),
            bars: 0,
        };
        let mut lines = source.trim_start_matches('\u{feff}').lines().enumerate();
        if lines.next().is_none_or(|(_, line)| line.trim() != "---") {
            return Err(ScoreError::Header { line: 1 });
        }
        let mut closed = false;
        let mut default_pattern = Pattern::Pop;
        for (index, line) in lines.by_ref() {
            let line = line.trim();
            if line == "---" {
                closed = true;
                break;
            }
            let Some((key, value)) = line.split_once(':') else {
                if line.is_empty() {
                    continue;
                }
                return Err(ScoreError::Header { line: index + 1 });
            };
            let value = value.trim();
            match key.trim() {
                "title" => {
                    if value.len() > limits.label_bytes {
                        return Err(ScoreError::Capacity);
                    }
                    score.title = score.intern(value);
                }
                "key" => {
                    score.key = [
                        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                    ]
                    .iter()
                    .position(|key| *key == value)
                    .ok_or(ScoreError::Header { line: index + 1 })?
                        as u8;
                }
                "bpm" => {
                    score.milli_bpm =
                        parse_tempo(value).ok_or(ScoreError::Tempo { line: index + 1 })?;
                }
                "time" => {
                    let meter = value.split_once('/').and_then(|(beats, unit)| {
                        Some(Meter {
                            beats: beats.trim().parse().ok()?,
                            unit: unit.trim().parse().ok()?,
                        })
                    });
                    score.meter = meter
                        .filter(|m| m.beats > 0 && m.beats <= 16 && [2, 4, 8, 16].contains(&m.unit))
                        .ok_or(ScoreError::Meter { line: index + 1 })?;
                }
                "strum" => {
                    default_pattern =
                        Pattern::parse(value).ok_or(ScoreError::Pattern { line: index + 1 })?;
                }
                _ => return Err(ScoreError::Header { line: index + 1 }),
            }
        }
        if !closed || score.title.is_empty() {
            return Err(ScoreError::Header { line: 1 });
        }
        for (index, line) in lines {
            let line = line.trim();
            let line_no = index + 1;
            if line.is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix("---").and_then(|s| s.strip_suffix("---")) {
                let mut parts = header.split('|');
                let name = parts.next().unwrap_or("").trim();
                if name.len() > limits.label_bytes {
                    return Err(ScoreError::Capacity);
                }
                let mut pattern = default_pattern;
                for part in parts {
                    let value = part
                        .trim()
                        .strip_prefix("strum:")
                        .ok_or(ScoreError::Header { line: line_no })?;
                    pattern = Pattern::parse(value.trim())
                        .ok_or(ScoreError::Pattern { line: line_no })?;
                }
                if score.sections.len() >= limits.sections {
                    return Err(ScoreError::Capacity);
                }
                let name = score.intern(name);
                score.sections.push(Section {
                    name,
                    pattern,
                    first_bar: score.bars,
                });
                continue;
            }
            if score.sections.is_empty() {
                if limits.sections == 0 {
                    return Err(ScoreError::Capacity);
                }
                score.sections.push(Section {
                    name: 0..0,
                    pattern: default_pattern,
                    first_bar: 0,
                });
            }
            let (lyrics, degrees) = line
                .rsplit_once('@')
                .ok_or(ScoreError::Bars { line: line_no })?;
            let mut groups = degrees.split_whitespace();
            for bar in lyrics.split('|') {
                let group = groups.next().ok_or(ScoreError::Bars { line: line_no })?;
                if usize::from(score.bars) >= limits.bars || score.bars == u16::MAX {
                    return Err(ScoreError::Capacity);
                }
                let slots = group.split('/').count();
                if slots > 16 || score.chords.len().saturating_add(slots) > limits.chords {
                    return Err(ScoreError::Capacity);
                }
                let bar = bar.trim();
                let accents = validate_accents(bar).ok_or(ScoreError::Accent { line: line_no })?;
                if slots > 1 && accents != slots {
                    return Err(ScoreError::Accent { line: line_no });
                }
                let mut start = 0;
                let mut next = bar
                    .char_indices()
                    .filter_map(|(i, ch)| (ch == '[').then_some(i));
                next.next();
                for (slot, degree) in group.split('/').enumerate() {
                    let degree = degree.strip_suffix('m').unwrap_or(degree);
                    let degree = degree
                        .parse::<u8>()
                        .ok()
                        .filter(|n| (1..=6).contains(n))
                        .ok_or(ScoreError::Degree { line: line_no })?;
                    let end = if slots == 1 || slot + 1 == slots {
                        bar.len()
                    } else {
                        next.next().unwrap_or(bar.len())
                    };
                    let text_start = score.text.len();
                    if end - start > limits.lyric_bytes {
                        return Err(ScoreError::Capacity);
                    }
                    score.text.extend(
                        bar[start..end]
                            .chars()
                            .filter(|ch| !['[', ']'].contains(ch)),
                    );
                    score.chords.push(Chord {
                        degree,
                        lyric: text_start..score.text.len(),
                        bar: score.bars,
                        slot: slot as u8,
                        slots: slots as u8,
                    });
                    start = end;
                }
                score.bars += 1;
            }
            if groups.next().is_some() {
                return Err(ScoreError::Bars { line: line_no });
            }
        }
        if score.bars == 0 {
            return Err(ScoreError::Empty);
        }
        Ok(score)
    }

    fn intern(&mut self, value: &str) -> Range<usize> {
        let start = self.text.len();
        self.text.push_str(value);
        start..self.text.len()
    }

    pub fn title(&self) -> &str {
        &self.text[self.title.clone()]
    }
    pub fn text(&self, range: Range<usize>) -> &str {
        &self.text[range]
    }
    pub fn chords(&self) -> &[Chord] {
        &self.chords
    }
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
    pub fn bars(&self) -> u16 {
        self.bars
    }

    pub fn section_at(&self, bar: u16) -> usize {
        self.sections
            .partition_point(|section| section.first_bar <= bar)
            .saturating_sub(1)
    }

    pub fn chord_at(&self, tick: u64) -> usize {
        let bar_ticks = u64::from(self.meter.beats) * super::TICKS_PER_BEAT;
        let tick = tick.min(u64::from(self.bars) * bar_ticks - 1);
        let bar = (tick / bar_ticks).min(u64::from(self.bars.saturating_sub(1))) as u16;
        let first = self.chords.partition_point(|chord| chord.bar < bar);
        let slots = self.chords[first].slots;
        first + ((tick % bar_ticks) * u64::from(slots) / bar_ticks) as usize
    }

    pub fn chord_name(&self, degree: u8) -> &'static str {
        const MAJOR: [&str; 12] = [
            "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
        ];
        const MINOR: [&str; 12] = [
            "Cm", "C#m", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "Abm", "Am", "Bbm", "Bm",
        ];
        const OFFSETS: [u8; 6] = [0, 2, 4, 5, 7, 9];
        let Some(offset) = degree
            .checked_sub(1)
            .and_then(|i| OFFSETS.get(usize::from(i)))
        else {
            return "?";
        };
        let index = usize::from((self.key + offset) % 12);
        if [2, 3, 6].contains(&degree) {
            MINOR[index]
        } else {
            MAJOR[index]
        }
    }
}

fn validate_accents(text: &str) -> Option<usize> {
    let mut count = 0;
    let mut open = false;
    for ch in text.chars() {
        match ch {
            '[' if !open => {
                count += 1;
                open = true;
            }
            ']' if open => open = false,
            '[' | ']' => return None,
            _ => {}
        }
    }
    (!open).then_some(count)
}

fn parse_tempo(text: &str) -> Option<u32> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole = whole.parse::<u32>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()? * 10_u32.pow(3 - fraction.len() as u32)
    };
    whole
        .checked_mul(1000)?
        .checked_add(fraction)
        .filter(|value| (1000..=999_000).contains(value))
}
