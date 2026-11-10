use super::{Score, ScoreError};
use crate::render::font::Font;

pub(super) const CHORD: crate::render::font::FontToken =
    crate::render::font::FontToken::Custom("prompter_chord_font");

pub(super) fn body(size: u16) -> Font {
    if size == 12 {
        open()
    } else {
        Font::from_mirx(
            "Prompter Pixel",
            24,
            include_bytes!("../assets/prompter-pixel-24.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("bundled 24px prompter font")
    }
}

pub(super) fn chord(size: u16) -> Font {
    Font::from_mirx(
        "Prompter Chord",
        size,
        if size == 36 {
            include_bytes!("../assets/prompter-chord-36.mirx")
        } else {
            include_bytes!("../assets/prompter-chord-48.mirx")
        },
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("bundled chord bitmap font")
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
