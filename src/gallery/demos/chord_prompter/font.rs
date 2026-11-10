use super::{Score, ScoreError};
use crate::render::font::Font;

pub(super) const CONTENT_SIZE: u16 = 24;

pub(super) fn body() -> Font {
    Font::from_mirx(
        "Prompter Pixel",
        12,
        include_bytes!("../assets/prompter-pixel.mirx"),
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("bundled prompter font")
}

pub(super) fn validate(score: &Score) -> Result<(), ScoreError> {
    let font = body();
    let text = score.title().chars().chain(
        score
            .chords()
            .iter()
            .flat_map(|chord| score.lyric(chord).chars()),
    );
    for ch in text {
        if !ch.is_whitespace() && font.map_char(ch).is_none() {
            return Err(ScoreError::UnsupportedCharacter(ch));
        }
    }
    Ok(())
}
