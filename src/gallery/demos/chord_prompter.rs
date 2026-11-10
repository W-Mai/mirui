//! A beat-driven chord and lyric display with a bounded song-sheet importer.

mod score;
mod transport;

pub use score::{Attack, Chord, Meter, Pattern, Score, ScoreError, ScoreLimits, Section, Stroke};
pub use transport::{Action, Boundary, CommandError, Playback, TICKS_PER_BEAT, Transport};

#[cfg(test)]
mod tests;
