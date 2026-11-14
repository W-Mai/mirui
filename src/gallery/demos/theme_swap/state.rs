use crate::prelude::{Color, ColorToken, Theme, ThemeId, ThemeInfo};

#[crate::component]
pub struct ThemeChoice(pub ThemeId);

pub const ACCENT: ColorToken = ColorToken::Tertiary;

pub fn dark_with_accent() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            "midnight-amber",
            "Midnight amber",
            "Graphite surfaces with a warm accent",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(16, 17, 19)),
            (ColorToken::OnSurface, Color::rgb(233, 234, 237)),
            (ColorToken::SurfaceVariant, Color::rgb(27, 28, 32)),
            (ColorToken::OnSurfaceVariant, Color::rgb(134, 138, 147)),
            (ColorToken::Outline, Color::rgb(48, 50, 56)),
            (ColorToken::Primary, Color::rgb(242, 184, 64)),
            (ColorToken::OnPrimary, Color::rgb(36, 25, 4)),
            (ColorToken::Secondary, Color::rgb(255, 214, 128)),
            (ColorToken::OnSecondary, Color::rgb(36, 25, 4)),
            (ACCENT, Color::rgb(255, 200, 60)),
            (ColorToken::OnTertiary, Color::rgb(36, 25, 4)),
        ])
}

pub fn light_with_accent() -> Theme {
    Theme::light()
        .with_info(ThemeInfo::new(
            "paper-rose",
            "Paper rose",
            "Warm paper surfaces with a rose accent",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(248, 246, 246)),
            (ColorToken::OnSurface, Color::rgb(32, 27, 29)),
            (ColorToken::SurfaceVariant, Color::rgb(238, 234, 235)),
            (ColorToken::OnSurfaceVariant, Color::rgb(140, 130, 134)),
            (ColorToken::Outline, Color::rgb(222, 215, 217)),
            (ColorToken::Primary, Color::rgb(196, 56, 88)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(160, 36, 66)),
            (ColorToken::OnSecondary, Color::rgb(255, 255, 255)),
            (ACCENT, Color::rgb(220, 60, 90)),
        ])
}

pub fn custom_theme() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            "deep-sea",
            "Deep sea",
            "Navy surfaces with cyan accents",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(10, 21, 31)),
            (ColorToken::OnSurface, Color::rgb(226, 236, 242)),
            (ColorToken::SurfaceVariant, Color::rgb(18, 34, 47)),
            (ColorToken::OnSurfaceVariant, Color::rgb(118, 146, 164)),
            (ColorToken::Outline, Color::rgb(38, 60, 78)),
            (ColorToken::Primary, Color::rgb(64, 196, 220)),
            (ColorToken::OnPrimary, Color::rgb(4, 24, 30)),
            (ColorToken::Secondary, Color::rgb(120, 222, 236)),
            (ColorToken::OnSecondary, Color::rgb(4, 24, 30)),
            (ColorToken::Success, Color::rgb(110, 205, 150)),
            (ACCENT, Color::rgb(140, 200, 220)),
            (ColorToken::OnTertiary, Color::rgb(4, 24, 30)),
        ])
}
