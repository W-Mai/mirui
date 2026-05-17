use crate::types::Color;

/// Color palette consumed by built-in widgets. World resource;
/// `App::new` inserts `Theme::default()`.
///
/// Token roles:
/// - `primary` / `on_primary`: main accent + text on it
/// - `secondary` / `on_secondary`, `tertiary` / `on_tertiary`: alt accents
/// - `surface` / `on_surface`: root background + primary text
/// - `surface_variant` / `on_surface_variant`: dim panel + secondary
///   / placeholder text
/// - `success` / `error`: state feedback
/// - `outline` / `shadow`: borders / elevation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub primary: Color,
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub on_surface_variant: Color,
    pub success: Color,
    pub error: Color,
    pub outline: Color,
    pub shadow: Color,
}

impl Theme {
    /// Dark palette; the default for `App::new`.
    pub const fn dark() -> Self {
        Self {
            primary: Color::rgb(88, 166, 255),
            on_primary: Color::rgb(255, 255, 255),
            secondary: Color::rgb(140, 200, 220),
            on_secondary: Color::rgb(20, 20, 30),
            tertiary: Color::rgb(200, 140, 220),
            on_tertiary: Color::rgb(20, 20, 30),
            surface: Color::rgb(20, 20, 30),
            on_surface: Color::rgb(220, 220, 230),
            surface_variant: Color::rgb(60, 60, 80),
            on_surface_variant: Color::rgb(120, 120, 140),
            success: Color::rgb(63, 185, 80),
            error: Color::rgb(220, 80, 80),
            outline: Color::rgb(80, 80, 100),
            shadow: Color::rgb(0, 0, 0),
        }
    }

    pub const fn light() -> Self {
        Self {
            primary: Color::rgb(0, 100, 200),
            on_primary: Color::rgb(255, 255, 255),
            secondary: Color::rgb(40, 120, 160),
            on_secondary: Color::rgb(255, 255, 255),
            tertiary: Color::rgb(140, 80, 180),
            on_tertiary: Color::rgb(255, 255, 255),
            surface: Color::rgb(248, 248, 250),
            on_surface: Color::rgb(20, 20, 30),
            surface_variant: Color::rgb(220, 220, 230),
            on_surface_variant: Color::rgb(120, 120, 140),
            success: Color::rgb(40, 160, 70),
            error: Color::rgb(200, 60, 60),
            outline: Color::rgb(180, 180, 200),
            shadow: Color::rgb(60, 60, 80),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_differ() {
        assert_ne!(Theme::dark().primary, Theme::light().primary);
        assert_ne!(Theme::dark().surface, Theme::light().surface);
    }

    #[test]
    fn default_is_dark() {
        assert_eq!(Theme::default(), Theme::dark());
    }

    #[test]
    fn dark_primary_pinned() {
        assert_eq!(Theme::dark().primary, Color::rgb(88, 166, 255));
    }
}
