use alloc::collections::BTreeMap;

use crate::ecs::World;
use crate::types::{Color, Fixed};

/// Token = role. State modifiers (Disabled / Hovered / …) belong on
/// [`WidgetState`], not in this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorToken {
    Primary,
    OnPrimary,
    Secondary,
    OnSecondary,
    Tertiary,
    OnTertiary,
    Surface,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    Success,
    Error,
    Outline,
    Shadow,
    Custom(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WidgetState {
    #[default]
    Enabled,
    Disabled,
    Hovered,
    Pressed,
    Error,
}

impl ColorToken {
    pub const fn custom(name: &'static str) -> Self {
        Self::Custom(name)
    }
}

/// A colour value that's either fixed or routed through a
/// [`ColorToken`]. Built-in widgets and `Style` carry `ThemedColor`
/// fields so user code mixes literals and tokens freely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemedColor {
    Raw(Color),
    Token(ColorToken),
}

impl ThemedColor {
    pub fn resolve(self, theme: &Theme) -> Color {
        match self {
            Self::Raw(c) => c,
            Self::Token(t) => theme.resolve(t),
        }
    }

    pub fn resolve_in(self, theme: &Theme, state: WidgetState) -> Color {
        match self {
            Self::Raw(c) => theme.blend_color_in(c, state),
            Self::Token(t) => theme.resolve_in(t, state),
        }
    }
}

impl From<Color> for ThemedColor {
    fn from(c: Color) -> Self {
        Self::Raw(c)
    }
}

impl From<ColorToken> for ThemedColor {
    fn from(t: ColorToken) -> Self {
        Self::Token(t)
    }
}

/// Magenta: paint when a `Custom` token isn't bound. Loud in any
/// palette so an unbound token shows up immediately.
const MISSING_TOKEN_FALLBACK: Color = Color::rgb(255, 0, 255);

/// Colour palette consumed by built-in widgets. World resource;
/// `App::new` inserts `Theme::default()`.
///
/// Token roles:
/// - `Primary` / `OnPrimary`: main accent + text on it
/// - `Secondary` / `OnSecondary`, `Tertiary` / `OnTertiary`: alt accents
/// - `Surface` / `OnSurface`: root background + primary text
/// - `SurfaceVariant` / `OnSurfaceVariant`: dim panel + secondary
///   / placeholder text
/// - `Success` / `Error`: state feedback
/// - `Outline` / `Shadow`: borders / elevation
#[derive(Clone, Debug)]
pub struct Theme {
    primary: Color,
    on_primary: Color,
    secondary: Color,
    on_secondary: Color,
    tertiary: Color,
    on_tertiary: Color,
    surface: Color,
    on_surface: Color,
    surface_variant: Color,
    on_surface_variant: Color,
    success: Color,
    error: Color,
    outline: Color,
    shadow: Color,
    extras: BTreeMap<&'static str, Color>,
}

impl Theme {
    /// Dark palette; the default for `App::new`.
    pub fn dark() -> Self {
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
            extras: BTreeMap::new(),
        }
    }

    pub fn light() -> Self {
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
            extras: BTreeMap::new(),
        }
    }

    pub fn resolve(&self, token: ColorToken) -> Color {
        match token {
            ColorToken::Primary => self.primary,
            ColorToken::OnPrimary => self.on_primary,
            ColorToken::Secondary => self.secondary,
            ColorToken::OnSecondary => self.on_secondary,
            ColorToken::Tertiary => self.tertiary,
            ColorToken::OnTertiary => self.on_tertiary,
            ColorToken::Surface => self.surface,
            ColorToken::OnSurface => self.on_surface,
            ColorToken::SurfaceVariant => self.surface_variant,
            ColorToken::OnSurfaceVariant => self.on_surface_variant,
            ColorToken::Success => self.success,
            ColorToken::Error => self.error,
            ColorToken::Outline => self.outline,
            ColorToken::Shadow => self.shadow,
            ColorToken::Custom(name) => self
                .extras
                .get(name)
                .copied()
                .unwrap_or(MISSING_TOKEN_FALLBACK),
        }
    }

    pub fn resolve_in(&self, token: ColorToken, state: WidgetState) -> Color {
        let base = self.resolve(token);
        match state {
            WidgetState::Enabled => base,
            WidgetState::Disabled => match token {
                ColorToken::OnSurface
                | ColorToken::OnSurfaceVariant
                | ColorToken::OnPrimary
                | ColorToken::OnSecondary
                | ColorToken::OnTertiary => self.surface.blend_with(base, Fixed::from_f32(0.38)),
                ColorToken::Primary
                | ColorToken::Secondary
                | ColorToken::Tertiary
                | ColorToken::SurfaceVariant => self
                    .surface
                    .blend_with(self.on_surface, Fixed::from_f32(0.12)),
                _ => base,
            },
            WidgetState::Hovered => base.blend_with(self.on_surface, Fixed::from_f32(0.08)),
            WidgetState::Pressed => base.blend_with(self.on_surface, Fixed::from_f32(0.12)),
            WidgetState::Error => base.blend_with(self.error, Fixed::from_f32(0.16)),
        }
    }

    pub fn blend_color_in(&self, color: Color, state: WidgetState) -> Color {
        match state {
            WidgetState::Enabled => color,
            WidgetState::Disabled => self.surface.blend_with(color, Fixed::from_f32(0.38)),
            WidgetState::Hovered => color.blend_with(self.on_surface, Fixed::from_f32(0.08)),
            WidgetState::Pressed => color.blend_with(self.on_surface, Fixed::from_f32(0.12)),
            WidgetState::Error => color.blend_with(self.error, Fixed::from_f32(0.16)),
        }
    }
}

impl Theme {
    /// Bind a colour to a token, builtin or custom.
    pub fn set(&mut self, token: ColorToken, color: Color) -> &mut Self {
        match token {
            ColorToken::Primary => self.primary = color,
            ColorToken::OnPrimary => self.on_primary = color,
            ColorToken::Secondary => self.secondary = color,
            ColorToken::OnSecondary => self.on_secondary = color,
            ColorToken::Tertiary => self.tertiary = color,
            ColorToken::OnTertiary => self.on_tertiary = color,
            ColorToken::Surface => self.surface = color,
            ColorToken::OnSurface => self.on_surface = color,
            ColorToken::SurfaceVariant => self.surface_variant = color,
            ColorToken::OnSurfaceVariant => self.on_surface_variant = color,
            ColorToken::Success => self.success = color,
            ColorToken::Error => self.error = color,
            ColorToken::Outline => self.outline = color,
            ColorToken::Shadow => self.shadow = color,
            ColorToken::Custom(name) => {
                self.extras.insert(name, color);
            }
        }
        self
    }

    /// Drop a `Custom` token. No-op for builtins (which always have a value).
    pub fn unset(&mut self, token: ColorToken) -> &mut Self {
        if let ColorToken::Custom(name) = token {
            self.extras.remove(name);
        }
        self
    }

    /// Owning chainable variant of `set` — `Theme::dark().with(Token, color)…`.
    pub fn with(mut self, token: ColorToken, color: Color) -> Self {
        self.set(token, color);
        self
    }

    pub fn with_many<I>(mut self, pairs: I) -> Self
    where
        I: IntoIterator<Item = (ColorToken, Color)>,
    {
        for (token, color) in pairs {
            self.set(token, color);
        }
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Free-function counterpart to `App::set_theme`, for handlers and
/// systems that don't have an `App` reference.
pub fn set_theme(world: &mut World, theme: Theme) {
    world.insert_resource(theme);
    if let Some(super::WidgetRoot(root)) = world.resource::<super::WidgetRoot>().copied() {
        super::dirty::mark_subtree_dirty(world, root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_primary_pinned() {
        assert_eq!(
            Theme::dark().resolve(ColorToken::Primary),
            Color::rgb(88, 166, 255),
        );
    }

    #[test]
    fn default_is_dark() {
        let d = Theme::default();
        let dark = Theme::dark();
        for token in [
            ColorToken::Primary,
            ColorToken::OnPrimary,
            ColorToken::Surface,
            ColorToken::OnSurface,
            ColorToken::Success,
        ] {
            assert_eq!(d.resolve(token), dark.resolve(token));
        }
    }

    #[test]
    fn custom_token_round_trip() {
        const BRAND: ColorToken = ColorToken::custom("brand_red");
        let mut t = Theme::dark();
        assert_eq!(t.resolve(BRAND), MISSING_TOKEN_FALLBACK);
        t.set(BRAND, Color::rgb(220, 60, 70));
        assert_eq!(t.resolve(BRAND), Color::rgb(220, 60, 70));
        t.unset(BRAND);
        assert_eq!(t.resolve(BRAND), MISSING_TOKEN_FALLBACK);
    }

    #[test]
    fn set_chain_returns_self() {
        const A: ColorToken = ColorToken::custom("a");
        const B: ColorToken = ColorToken::custom("b");
        let mut t = Theme::dark();
        t.set(A, Color::rgb(1, 0, 0)).set(B, Color::rgb(0, 1, 0));
        assert_eq!(t.resolve(A), Color::rgb(1, 0, 0));
        assert_eq!(t.resolve(B), Color::rgb(0, 1, 0));
    }

    #[test]
    fn themed_color_raw_ignores_theme() {
        let dark = Theme::dark();
        let light = Theme::light();
        let red = ThemedColor::Raw(Color::rgb(255, 0, 0));
        assert_eq!(red.resolve(&dark), Color::rgb(255, 0, 0));
        assert_eq!(red.resolve(&light), Color::rgb(255, 0, 0));
    }

    #[test]
    fn themed_color_token_follows_theme() {
        let dark = Theme::dark();
        let light = Theme::light();
        let primary = ThemedColor::Token(ColorToken::Primary);
        assert_eq!(primary.resolve(&dark), Color::rgb(88, 166, 255));
        assert_eq!(primary.resolve(&light), Color::rgb(0, 100, 200));
    }

    #[test]
    fn from_color_and_token() {
        let from_color: ThemedColor = Color::rgb(1, 2, 3).into();
        assert!(matches!(from_color, ThemedColor::Raw(_)));
        let from_token: ThemedColor = ColorToken::Surface.into();
        assert!(matches!(
            from_token,
            ThemedColor::Token(ColorToken::Surface)
        ));
    }

    #[test]
    fn set_builtin_overrides_resolve() {
        let mut t = Theme::dark();
        t.set(ColorToken::Primary, Color::rgb(255, 0, 0));
        assert_eq!(t.resolve(ColorToken::Primary), Color::rgb(255, 0, 0));
    }

    #[test]
    fn unset_builtin_is_noop() {
        let mut t = Theme::dark();
        let before = t.resolve(ColorToken::Primary);
        t.unset(ColorToken::Primary);
        assert_eq!(t.resolve(ColorToken::Primary), before);
    }

    #[test]
    fn with_chain_owning() {
        const ACCENT: ColorToken = ColorToken::custom("accent");
        let t = Theme::dark()
            .with(ColorToken::Primary, Color::rgb(255, 0, 0))
            .with(ACCENT, Color::rgb(0, 200, 0));
        assert_eq!(t.resolve(ColorToken::Primary), Color::rgb(255, 0, 0));
        assert_eq!(t.resolve(ACCENT), Color::rgb(0, 200, 0));
        // untouched builtins keep their dark default
        assert_eq!(t.resolve(ColorToken::Surface), Theme::dark().surface);
    }

    #[test]
    fn with_many_iterates_all_pairs() {
        let pairs = [
            (ColorToken::Primary, Color::rgb(1, 1, 1)),
            (ColorToken::Surface, Color::rgb(2, 2, 2)),
            (ColorToken::custom("brand"), Color::rgb(3, 3, 3)),
        ];
        let t = Theme::dark().with_many(pairs);
        assert_eq!(t.resolve(ColorToken::Primary), Color::rgb(1, 1, 1));
        assert_eq!(t.resolve(ColorToken::Surface), Color::rgb(2, 2, 2));
        assert_eq!(t.resolve(ColorToken::custom("brand")), Color::rgb(3, 3, 3));
    }

    #[test]
    fn resolve_in_enabled_falls_through() {
        let t = Theme::dark();
        for tok in [
            ColorToken::Primary,
            ColorToken::OnSurface,
            ColorToken::SurfaceVariant,
            ColorToken::Outline,
        ] {
            assert_eq!(t.resolve_in(tok, WidgetState::Enabled), t.resolve(tok));
        }
    }

    #[test]
    fn resolve_in_disabled_text_blends_38() {
        let t = Theme::dark();
        let expected = t.surface.blend_with(t.on_surface, Fixed::from_f32(0.38));
        assert_eq!(
            t.resolve_in(ColorToken::OnSurface, WidgetState::Disabled),
            expected
        );
    }

    #[test]
    fn resolve_in_disabled_container_blends_12() {
        let t = Theme::dark();
        let expected = t.surface.blend_with(t.on_surface, Fixed::from_f32(0.12));
        assert_eq!(
            t.resolve_in(ColorToken::Primary, WidgetState::Disabled),
            expected
        );
        assert_eq!(
            t.resolve_in(ColorToken::SurfaceVariant, WidgetState::Disabled),
            expected
        );
    }

    #[test]
    fn resolve_in_outline_unchanged_when_disabled() {
        let t = Theme::dark();
        assert_eq!(
            t.resolve_in(ColorToken::Outline, WidgetState::Disabled),
            t.resolve(ColorToken::Outline),
        );
    }

    #[test]
    fn themed_color_raw_passes_through_when_enabled() {
        let t = Theme::dark();
        let raw = ThemedColor::Raw(Color::rgb(7, 8, 9));
        assert_eq!(
            raw.resolve_in(&t, WidgetState::Enabled),
            Color::rgb(7, 8, 9)
        );
    }

    #[test]
    fn themed_color_raw_blends_38_when_disabled() {
        let t = Theme::dark();
        let raw_color = Color::rgb(248, 81, 73);
        let expected = t.surface.blend_with(raw_color, Fixed::from_f32(0.38));
        assert_eq!(
            ThemedColor::Raw(raw_color).resolve_in(&t, WidgetState::Disabled),
            expected,
        );
    }

    #[test]
    fn resolve_in_hovered_overlays_8_percent() {
        let t = Theme::dark();
        let base = t.resolve(ColorToken::Primary);
        let expected = base.blend_with(t.on_surface, Fixed::from_f32(0.08));
        assert_eq!(
            t.resolve_in(ColorToken::Primary, WidgetState::Hovered),
            expected,
        );
    }

    #[test]
    fn resolve_in_pressed_overlays_12_percent() {
        let t = Theme::dark();
        let base = t.resolve(ColorToken::Primary);
        let expected = base.blend_with(t.on_surface, Fixed::from_f32(0.12));
        assert_eq!(
            t.resolve_in(ColorToken::Primary, WidgetState::Pressed),
            expected,
        );
    }

    #[test]
    fn resolve_in_error_overlays_error_token() {
        let t = Theme::dark();
        let base = t.resolve(ColorToken::Primary);
        let expected = base.blend_with(t.error, Fixed::from_f32(0.16));
        assert_eq!(
            t.resolve_in(ColorToken::Primary, WidgetState::Error),
            expected,
        );
    }

    #[test]
    fn raw_blends_in_hover_press_error() {
        let t = Theme::dark();
        let raw = Color::rgb(100, 200, 50);
        let h = ThemedColor::Raw(raw).resolve_in(&t, WidgetState::Hovered);
        assert_eq!(h, raw.blend_with(t.on_surface, Fixed::from_f32(0.08)));
        let p = ThemedColor::Raw(raw).resolve_in(&t, WidgetState::Pressed);
        assert_eq!(p, raw.blend_with(t.on_surface, Fixed::from_f32(0.12)));
        let er = ThemedColor::Raw(raw).resolve_in(&t, WidgetState::Error);
        assert_eq!(er, raw.blend_with(t.error, Fixed::from_f32(0.16)));
    }
}
