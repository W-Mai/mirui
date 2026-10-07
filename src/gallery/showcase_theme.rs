//! Shared presentation palettes for the Gallery showcase family.

use crate::app::{App, RendererFactory};
use crate::surface::Surface;
use crate::types::Color;
use crate::ui::theme::{ColorToken, Theme, ThemeInfo};

pub const DARK_ID: &str = "showcase-dark";
pub const LIGHT_ID: &str = "showcase-light";

pub fn dark() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            DARK_ID,
            "Showcase Dark",
            "Deep navy surfaces with cyan, blue, violet, and amber signals",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(6, 16, 23)),
            (ColorToken::OnSurface, Color::rgb(232, 240, 248)),
            (ColorToken::SurfaceVariant, Color::rgb(17, 30, 49)),
            (ColorToken::OnSurfaceVariant, Color::rgb(139, 163, 188)),
            (ColorToken::Outline, Color::rgb(47, 73, 101)),
            (ColorToken::Primary, Color::rgb(86, 226, 205)),
            (ColorToken::OnPrimary, Color::rgb(6, 16, 23)),
            (ColorToken::Secondary, Color::rgb(102, 161, 255)),
            (ColorToken::OnSecondary, Color::rgb(6, 16, 23)),
            (ColorToken::Tertiary, Color::rgb(179, 132, 255)),
            (ColorToken::OnTertiary, Color::rgb(6, 16, 23)),
            (ColorToken::Success, Color::rgb(255, 198, 92)),
            (ColorToken::Error, Color::rgb(244, 96, 112)),
            (ColorToken::Shadow, Color::rgb(0, 5, 12)),
        ])
}

pub fn light() -> Theme {
    Theme::light()
        .with_info(ThemeInfo::new(
            LIGHT_ID,
            "Showcase Light",
            "Cool paper surfaces with deep teal, blue, violet, and amber signals",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(246, 249, 252)),
            (ColorToken::OnSurface, Color::rgb(18, 33, 49)),
            (ColorToken::SurfaceVariant, Color::rgb(226, 235, 244)),
            (ColorToken::OnSurfaceVariant, Color::rgb(91, 116, 139)),
            (ColorToken::Outline, Color::rgb(151, 174, 195)),
            (ColorToken::Primary, Color::rgb(16, 132, 121)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(48, 104, 196)),
            (ColorToken::OnSecondary, Color::rgb(255, 255, 255)),
            (ColorToken::Tertiary, Color::rgb(119, 78, 180)),
            (ColorToken::OnTertiary, Color::rgb(255, 255, 255)),
            (ColorToken::Success, Color::rgb(174, 103, 0)),
            (ColorToken::Error, Color::rgb(190, 54, 78)),
            (ColorToken::Shadow, Color::rgb(48, 65, 82)),
        ])
}

/// Register the showcase palettes and select the dark theme.
pub fn install<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.register_theme(dark());
    app.register_theme(light());
    app.set_theme(DARK_ID).expect("showcase theme registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showcase_installation_registers_both_palettes_and_selects_dark() {
        let mut app = App::headless(1, 1);
        install(&mut app);

        assert!(app.themes().get(DARK_ID).is_some());
        assert!(app.themes().get(LIGHT_ID).is_some());
        assert_eq!(app.theme().id().as_str(), DARK_ID);
    }

    #[test]
    fn showcase_palettes_have_stable_ids_and_contrast() {
        for theme in [dark(), light()] {
            assert!(matches!(theme.id().as_str(), DARK_ID | LIGHT_ID));
            assert_ne!(
                theme.resolve(ColorToken::Surface),
                theme.resolve(ColorToken::OnSurface)
            );
            assert_ne!(
                theme.resolve(ColorToken::Primary),
                theme.resolve(ColorToken::OnPrimary)
            );
        }
    }
}
