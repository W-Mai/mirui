//! A beat-driven chord and lyric display with a bounded song-sheet importer.

mod composition;
mod font;
mod link;
mod motion;
mod runtime;
mod score;
mod state;
mod strips;
mod transport;

pub use link::{DeviceLink, LinkError, checksum};
pub use motion::BarMotion;
pub use runtime::install;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use score::{
    Attack, Chord, Lyric, LyricSegment, Meter, Pattern, Score, ScoreError, ScoreLimits, Section,
    Stroke,
};
pub use state::{Session, SessionHandle};
pub use transport::{Action, Boundary, CommandError, Playback, TICKS_PER_BEAT, Transport};

pub const VIEWPORT: (u16, u16) = (320, 240);
pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(128, 128);
pub const PRACTICE: &str = include_str!("chord_prompter/chord-practice.md");

#[cfg(test)]
mod tests;
