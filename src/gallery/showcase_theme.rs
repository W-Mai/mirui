//! Shared presentation palettes for the Gallery showcase family.

use crate::app::{App, RendererFactory};
use crate::surface::Surface;
use crate::types::Color;
use crate::ui::theme::{ColorToken, Theme, ThemeInfo};

pub const DARK_ID: &str = "showcase-dark";
pub const LIGHT_ID: &str = "showcase-light";
pub const SAMPLE_DARK_ID: &str = "sample-dark";
pub const SAMPLE_LIGHT_ID: &str = "sample-light";

pub fn dark() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            DARK_ID,
            "Showcase Dark",
            "Neutral graphite panels with the blue of the mirui wordmark",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(19, 20, 22)),
            (ColorToken::OnSurface, Color::rgb(236, 237, 239)),
            (ColorToken::SurfaceVariant, Color::rgb(28, 29, 33)),
            (ColorToken::OnSurfaceVariant, Color::rgb(142, 145, 152)),
            (ColorToken::Outline, Color::rgb(50, 52, 58)),
            (ColorToken::Primary, Color::rgb(111, 130, 255)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(150, 156, 168)),
            (ColorToken::OnSecondary, Color::rgb(19, 20, 22)),
            (ColorToken::Tertiary, Color::rgb(160, 140, 230)),
            (ColorToken::OnTertiary, Color::rgb(19, 20, 22)),
            (ColorToken::Success, Color::rgb(63, 185, 80)),
            (ColorToken::Error, Color::rgb(255, 106, 90)),
            (ColorToken::Shadow, Color::rgb(0, 0, 0)),
        ])
}

pub fn light() -> Theme {
    Theme::light()
        .with_info(ThemeInfo::new(
            LIGHT_ID,
            "Showcase Light",
            "Neutral paper panels with the same wordmark blue",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(242, 243, 245)),
            (ColorToken::OnSurface, Color::rgb(28, 30, 34)),
            (ColorToken::SurfaceVariant, Color::rgb(255, 255, 255)),
            (ColorToken::OnSurfaceVariant, Color::rgb(107, 112, 121)),
            (ColorToken::Outline, Color::rgb(220, 222, 227)),
            (ColorToken::Primary, Color::rgb(52, 82, 255)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(92, 99, 112)),
            (ColorToken::OnSecondary, Color::rgb(255, 255, 255)),
            (ColorToken::Tertiary, Color::rgb(112, 88, 196)),
            (ColorToken::OnTertiary, Color::rgb(255, 255, 255)),
            (ColorToken::Success, Color::rgb(26, 127, 55)),
            (ColorToken::Error, Color::rgb(207, 34, 46)),
            (ColorToken::Shadow, Color::rgb(57, 66, 76)),
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

/// Graphite for the small API samples: tonal greys with a trace of blue, the
/// brightest grey as the one action colour and no hue besides two muted signals.
pub fn sample_dark() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            SAMPLE_DARK_ID,
            "Sample Dark",
            "Tonal graphite; the action is the brightest grey",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(16, 17, 19)),
            (ColorToken::OnSurface, Color::rgb(233, 234, 237)),
            (ColorToken::SurfaceVariant, Color::rgb(25, 26, 29)),
            (ColorToken::OnSurfaceVariant, Color::rgb(134, 138, 147)),
            (ColorToken::Outline, Color::rgb(44, 46, 51)),
            (ColorToken::Primary, Color::rgb(233, 234, 237)),
            (ColorToken::OnPrimary, Color::rgb(16, 17, 19)),
            (ColorToken::Secondary, Color::rgb(160, 164, 173)),
            (ColorToken::OnSecondary, Color::rgb(16, 17, 19)),
            (ColorToken::Tertiary, Color::rgb(98, 104, 116)),
            (ColorToken::OnTertiary, Color::rgb(233, 234, 237)),
            (ColorToken::Success, Color::rgb(104, 196, 150)),
            (ColorToken::Error, Color::rgb(238, 122, 112)),
            (ColorToken::Shadow, Color::rgb(0, 0, 0)),
        ])
}

/// The paper side of the same graphite: white cards on a warm-neutral page.
pub fn sample_light() -> Theme {
    Theme::light()
        .with_info(ThemeInfo::new(
            SAMPLE_LIGHT_ID,
            "Sample Light",
            "Tonal paper; the action is the darkest grey",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(246, 246, 247)),
            (ColorToken::OnSurface, Color::rgb(23, 24, 27)),
            (ColorToken::SurfaceVariant, Color::rgb(236, 237, 240)),
            (ColorToken::OnSurfaceVariant, Color::rgb(111, 115, 124)),
            (ColorToken::Outline, Color::rgb(216, 218, 224)),
            (ColorToken::Primary, Color::rgb(23, 24, 27)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(88, 92, 101)),
            (ColorToken::OnSecondary, Color::rgb(255, 255, 255)),
            (ColorToken::Tertiary, Color::rgb(160, 164, 173)),
            (ColorToken::OnTertiary, Color::rgb(23, 24, 27)),
            (ColorToken::Success, Color::rgb(30, 140, 95)),
            (ColorToken::Error, Color::rgb(205, 60, 50)),
            (ColorToken::Shadow, Color::rgb(57, 66, 76)),
        ])
}

/// For the small API samples: the graphite palettes and the clean typeface of
/// the other demos, in place of the default theme and the 8-bit pixel face.
pub(in crate::gallery) fn install_sample<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.register_theme(sample_dark());
    app.register_theme(sample_light());
    app.set_theme(SAMPLE_DARK_ID)
        .expect("sample theme registered");
    app.add_plugin(super::play::font::PlayFontPlugin);
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

    #[test]
    fn sample_palettes_are_graphite_with_the_action_as_the_brightest_grey() {
        for theme in [sample_dark(), sample_light()] {
            let action = theme.resolve(ColorToken::Primary);
            let on_action = theme.resolve(ColorToken::OnPrimary);
            // Neutral to the eye: no channel strays far from the others.
            for color in [action, on_action, theme.resolve(ColorToken::Surface)] {
                assert!(color.r.abs_diff(color.b) <= 4, "{color:?}");
            }
            assert_ne!(action, on_action);
        }
        let mut app = App::headless(1, 1);
        super::install_sample(&mut app);
        assert_eq!(app.theme().id().as_str(), SAMPLE_DARK_ID);
        assert!(app.themes().get(SAMPLE_LIGHT_ID).is_some());
    }
}
