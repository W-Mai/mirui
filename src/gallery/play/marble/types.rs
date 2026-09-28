use crate::types::Fixed64;

pub(crate) const MAX_BALLS: usize = 8;
pub(crate) const MAX_PADS: usize = 6;
pub(crate) const MAX_PARTICLES: usize = 24;
pub(crate) const MAX_RINGS: usize = 8;
pub(crate) const TRAIL_LEN: usize = 6;

pub(crate) const PALETTE: [u32; 6] = [0xb4eabd, 0xc6b0ef, 0xeed984, 0xeeac8b, 0xa0d2e8, 0xd5eba0];
pub(crate) const PAD_PITCHES: [u8; 10] = [60, 62, 64, 67, 69, 72, 74, 76, 79, 81];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Vec2 {
    pub(crate) x: Fixed64,
    pub(crate) y: Fixed64,
}

impl Vec2 {
    pub(crate) const fn new(x: i64, y: i64) -> Self {
        Self {
            x: Fixed64::from_int(x),
            y: Fixed64::from_int(y),
        }
    }

    pub(super) const fn fixed(x: Fixed64, y: Fixed64) -> Self {
        Self { x, y }
    }

    pub(super) fn length(self) -> Fixed64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[inline]
    pub(super) fn length_below(self, threshold: Fixed64) -> Option<Fixed64> {
        let squared = self.x * self.x + self.y * self.y;
        // Keep the rounded boundary for the exact sqrt comparison.
        if squared > threshold * threshold {
            return None;
        }
        let distance = squared.sqrt();
        (distance < threshold).then_some(distance)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Page {
    Play,
    Edit,
    Scenes,
    Settings,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Theme {
    pub(crate) name: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) bg: u32,
    pub(crate) panel: u32,
    pub(crate) accent: u32,
    pub(crate) gravity: Fixed64,
}

pub(crate) const THEMES: [Theme; 3] = [
    Theme {
        name: "DAYDREAM",
        subtitle: "SOFT GREEN / NORMAL",
        bg: 0x17221c,
        panel: 0x222f25,
        accent: 0xd9f88a,
        gravity: Fixed64::from_ratio(80, 100),
    },
    Theme {
        name: "AFTER HOURS",
        subtitle: "BLUE GREY / LIGHT",
        bg: 0x1b2029,
        panel: 0x29303b,
        accent: 0xcfbbee,
        gravity: Fixed64::from_ratio(45, 100),
    },
    Theme {
        name: "ZERO GRAVITY",
        subtitle: "COOL BLUE / FLOAT",
        bg: 0x17242a,
        panel: 0x22343b,
        accent: 0xa9dfeb,
        gravity: Fixed64::from_ratio(8, 100),
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pad {
    pub(crate) id: u32,
    pub(crate) letter: u8,
    pub(crate) pos: Vec2,
    pub(crate) radius: Fixed64,
    pub(crate) color: u32,
    pub(crate) bounce: Fixed64,
    pub(crate) pulse: Fixed64,
    pub(crate) pitch: u8,
    pub(crate) timbre: PadTimbre,
    pub(super) last_hit: Fixed64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum PadTimbre {
    Mallet,
    Synth,
    Bass,
    Drum,
}

impl PadTimbre {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Mallet => "MALLET",
            Self::Synth => "SYNTH",
            Self::Bass => "BASS",
            Self::Drum => "DRUM",
        }
    }

    pub(super) const fn next(self) -> Self {
        match self {
            Self::Mallet => Self::Synth,
            Self::Synth => Self::Bass,
            Self::Bass => Self::Drum,
            Self::Drum => Self::Mallet,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ball {
    pub(crate) pos: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) color: u32,
    pub(crate) trail: [Vec2; TRAIL_LEN],
    pub(crate) trail_len: usize,
    pub(super) trail_clock: Fixed64,
    pub(super) cooldown: [Fixed64; MAX_PADS],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    pub(crate) pos: Vec2,
    pub(crate) radius: Fixed64,
    pub(crate) age: Fixed64,
    pub(crate) color: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Particle {
    pub(crate) pos: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) age: Fixed64,
    pub(crate) color: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MarbleSound {
    Pad {
        slot: u8,
        pitch: u8,
        timbre: PadTimbre,
        gain: u8,
        delay_ms: u16,
    },
}
