use super::{Score, ScoreError};
use crate::render::font::Font;

pub(super) const CHORD: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("prompter_chord_font");

pub(super) fn body() -> Font {
    open()
}

pub(super) fn chord() -> Font {
    Font::from_mirx(
        "Prompter Chord",
        36,
        include_bytes!("../assets/prompter-chord.mirx"),
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("bundled chord bitmap strikes")
}

pub(super) fn open() -> Font {
    Font::from_mirx(
        "Prompter Pixel",
        12,
        include_bytes!("../assets/prompter-pixel.mirx"),
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("bundled prompter font")
}

pub(super) fn validate(score: &Score) -> Result<(), ScoreError> {
    let font = open();
    let text = score.title().chars().chain(
        score
            .chords()
            .iter()
            .flat_map(|chord| score.text(chord.lyric.clone()).chars()),
    );
    for ch in text {
        if !ch.is_whitespace() && font.map_char(ch).is_none() {
            return Err(ScoreError::UnsupportedCharacter(ch));
        }
    }
    Ok(())
}
