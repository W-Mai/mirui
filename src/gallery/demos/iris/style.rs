use crate::prelude::{Color, ColorToken};
use crate::ui::Theme;
use crate::ui::theme::ThemeInfo;
use crate::ui::widgets::{ParagraphStyle, TextOverflow, TextWrap};

pub(super) const BG: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
/// Selected segment of a segmented control.
pub(super) const RAISED: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
/// The iris red of the mirui mark, also the capture colour.
pub(super) const SEE: ColorToken = ColorToken::Primary;
/// Connection and readiness.
pub(super) const OK: ColorToken = ColorToken::Success;
/// Clipped shadows or highlights.
pub(super) const CLIP: ColorToken = ColorToken::Error;

pub use super::{DARK_ID, LIGHT_ID};

pub fn dark() -> Theme {
    Theme::dark()
        .with_info(ThemeInfo::new(
            DARK_ID,
            "Iris Dark",
            "Neutral graphite panels with the red iris of the mirui mark",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(19, 20, 22)),
            (ColorToken::OnSurface, Color::rgb(236, 237, 239)),
            (ColorToken::SurfaceVariant, Color::rgb(28, 29, 33)),
            (ColorToken::OnSurfaceVariant, Color::rgb(142, 145, 152)),
            (ColorToken::Outline, Color::rgb(46, 48, 53)),
            (ColorToken::Primary, Color::rgb(229, 83, 59)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(111, 130, 255)),
            (ColorToken::OnSecondary, Color::rgb(19, 20, 22)),
            (ColorToken::Tertiary, Color::rgb(155, 107, 255)),
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
            "Iris Light",
            "Neutral paper panels with the same red iris",
        ))
        .with_many([
            (ColorToken::Surface, Color::rgb(242, 243, 245)),
            (ColorToken::OnSurface, Color::rgb(28, 30, 34)),
            (ColorToken::SurfaceVariant, Color::rgb(255, 255, 255)),
            (ColorToken::OnSurfaceVariant, Color::rgb(107, 112, 121)),
            (ColorToken::Outline, Color::rgb(223, 225, 229)),
            (ColorToken::Primary, Color::rgb(207, 67, 43)),
            (ColorToken::OnPrimary, Color::rgb(255, 255, 255)),
            (ColorToken::Secondary, Color::rgb(52, 82, 255)),
            (ColorToken::OnSecondary, Color::rgb(255, 255, 255)),
            (ColorToken::Tertiary, Color::rgb(119, 78, 220)),
            (ColorToken::OnTertiary, Color::rgb(255, 255, 255)),
            (ColorToken::Success, Color::rgb(26, 127, 55)),
            (ColorToken::Error, Color::rgb(207, 34, 46)),
            (ColorToken::Shadow, Color::rgb(57, 66, 76)),
        ])
}

/// Dark lenses keep a black barrel; light themes switch to brushed silver.
pub(super) fn is_dark(theme: &Theme) -> bool {
    let surface = theme.resolve(BG);
    u16::from(surface.r) + u16::from(surface.g) + u16::from(surface.b) < 384
}

pub(super) fn label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        overflow: TextOverflow::Clip,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}
