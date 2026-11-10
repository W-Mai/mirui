//! A beat-driven chord and lyric display with a bounded song-sheet importer.

mod composition;
mod font;
mod link;
mod runtime;
mod score;
mod state;
mod strips;
mod transport;

pub use link::{DeviceLink, LinkError, checksum};
pub use runtime::install;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use score::{Attack, Chord, Meter, Pattern, Score, ScoreError, ScoreLimits, Section, Stroke};
pub use state::{Session, SessionHandle};
pub use transport::{Action, Boundary, CommandError, Playback, TICKS_PER_BEAT, Transport};

pub const VIEWPORT: (u16, u16) = (360, 480);
pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::range(128, 128, 480, 640);
pub const PRACTICE: &str = "---\ntitle: Midnight Session\nkey: C\nbpm: 96\ntime: 4/4\nstrum: pop\n---\n--- Verse ---\n[Keep] the rhythm | [Let] it ring | [Take] a breath | [Start] again @ 1 5 6m 4\n--- Chorus | strum:folk ---\n[One] more [turn] | [Find] your way | [Stay] together | [Home] again @ 1/5 6m 4 1\n";

#[cfg(test)]
mod tests;
