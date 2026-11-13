//! Where to play a pentatonic scale on a guitar neck, for the bars of a solo.
//!
//! A pentatonic scale has five notes, and on a six string guitar in standard
//! tuning it falls into five boxes that climb the neck: each string holds two
//! notes, and the notes of a box are twelve consecutive notes of the scale,
//! two to a string from the low E up. The pattern repeats every twelve frets,
//! so a box has a low home and, on a neck with enough frets, one an octave
//! higher.

/// Strings of the guitar. Index 0 is the low E, the sixth string.
pub const STRINGS: usize = 6;
/// Frets of the neck the high home of a box has to fit on.
pub const NECK_FRETS: u8 = 22;
/// Positions the neck is cut into.
pub const POSITIONS: u8 = 5;
/// Pitch of each open string, in semitones above the low E.
const OPEN: [u8; STRINGS] = [0, 5, 10, 15, 19, 24];
/// Pitch class (0 is C) of the low E.
const LOW_E: u8 = 4;
/// Notes of the scale in one octave.
const NOTES: usize = 5;

/// Which five notes the solo draws on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Scale {
    /// The major pentatonic of the key, which is also the minor pentatonic of
    /// its relative minor. The first box has that minor tonic on the sixth
    /// string, as guitarists number them: A minor in the key of C.
    #[default]
    Pentatonic,
    /// The minor pentatonic that starts on the key itself, for a blues or
    /// rock solo over a major key.
    Minor,
}

impl Scale {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "pent" => Some(Self::Pentatonic),
            "minor" => Some(Self::Minor),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Pentatonic => "pent",
            Self::Minor => "minor",
        }
    }

    /// Pitch classes of the five notes in `key`, a pitch class itself.
    fn notes(self, key: u8) -> [u8; NOTES] {
        let steps = match self {
            Self::Pentatonic => [0, 2, 4, 7, 9],
            Self::Minor => [0, 3, 5, 7, 10],
        };
        steps.map(|step| (key + step) % 12)
    }

    /// The pitch class the first box is named for.
    fn home(self, key: u8) -> u8 {
        match self {
            Self::Pentatonic => (key + 9) % 12,
            Self::Minor => key % 12,
        }
    }
}

/// A box on the neck: the fret and pitch class of two notes on every string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fretboard {
    /// Frets of the two notes on each string, lowest string first.
    pub frets: [[u8; 2]; STRINGS],
    /// Pitch classes of those notes, 0 for C.
    pub notes: [[u8; 2]; STRINGS],
    /// The lowest fret of the box.
    pub low: u8,
    /// The lowest fret of the same box an octave up, when the neck has it.
    pub high: Option<u8>,
}

/// What a note of the box is to the chord that sounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// The chord's root.
    Root,
    /// Another note of the chord's triad.
    Tone,
    /// Only a note of the scale.
    Scale,
}

/// The triad of a chord as pitch classes: its root and a mask over the twelve.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Tones {
    pub root: u8,
    pub mask: u16,
}

impl Tones {
    pub fn new(root: u8, minor: bool) -> Self {
        let root = root % 12;
        let third = if minor { 3 } else { 4 };
        let bit = |step: u8| 1u16 << ((root + step) % 12);
        Self {
            root,
            mask: bit(0) | bit(third) | bit(7),
        }
    }

    pub fn role(self, note: u8) -> Role {
        if note == self.root {
            Role::Root
        } else if self.mask & (1 << note) != 0 {
            Role::Tone
        } else {
            Role::Scale
        }
    }
}

/// The box a solo is played in: `position` counts from 1 to 5 up the neck from
/// the box named for the scale's home note.
pub fn fretboard(key: u8, scale: Scale, position: u8) -> Fretboard {
    let mut residues = scale.notes(key).map(|note| (note + 12 - LOW_E) % 12);
    residues.sort_unstable();
    let home = (scale.home(key) + 12 - LOW_E) % 12;
    let home_index = residues.iter().position(|r| *r == home).unwrap_or(0);
    let start = (home_index + usize::from(position.clamp(1, POSITIONS) - 1)) % NOTES;
    // Pitches of the scale, from the low E up, three octaves of them.
    let pitch = |index: usize| residues[index % NOTES] + 12 * (index / NOTES) as u8;
    let mut frets = [[0; 2]; STRINGS];
    let mut notes = [[0; 2]; STRINGS];
    // A box that reaches below the open strings is played an octave up.
    for octave in [0, NOTES] {
        let mut fits = true;
        for string in 0..STRINGS {
            for slot in 0..2 {
                let pitch = pitch(start + octave + 2 * string + slot);
                notes[string][slot] = (pitch + LOW_E) % 12;
                match pitch.checked_sub(OPEN[string]) {
                    Some(fret) => frets[string][slot] = fret,
                    None => fits = false,
                }
            }
        }
        if fits {
            break;
        }
    }
    let low = frets.iter().flatten().copied().min().unwrap_or(0);
    let top = frets.iter().flatten().copied().max().unwrap_or(0);
    Fretboard {
        frets,
        notes,
        low,
        high: (top + 12 <= NECK_FRETS).then_some(low + 12),
    }
}

#[cfg(test)]
mod tests;
