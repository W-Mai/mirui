use crate::prelude::{Color, ColorToken, Theme, ThemeId, ThemeInfo};

pub struct ThemeChoice(pub ThemeId);

pub const ACCENT: ColorToken = ColorToken::Tertiary;

pub fn dark_with_accent() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            "midnight-amber",
            "Midnight Amber",
            "Dark surfaces with a warm accent",
        ))
        .with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    Theme::light()
        .with_info(ThemeInfo::new(
            "paper-rose",
            "Paper Rose",
            "Light surfaces with a rose accent",
        ))
        .with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            "plum",
            "Plum",
            "Deep plum surfaces with cyan accents",
        ))
        .with_many([
            (ColorToken::Primary, Color::rgb(255, 105, 180)),
            (ColorToken::OnPrimary, Color::rgb(20, 20, 30)),
            (ColorToken::Success, Color::rgb(255, 200, 60)),
            (ColorToken::Surface, Color::rgb(38, 28, 50)),
            (ColorToken::SurfaceVariant, Color::rgb(70, 50, 90)),
            (ColorToken::OnSurface, Color::rgb(245, 235, 255)),
            (ColorToken::OnSurfaceVariant, Color::rgb(180, 150, 200)),
            (ACCENT, Color::rgb(140, 200, 220)),
        ])
}
