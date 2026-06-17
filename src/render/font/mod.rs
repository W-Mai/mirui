//! Font resource management — registry, token, provider trait.
//!
//! `FontRegistry` is the World resource that maps a [`FontToken`] to a
//! concrete [`Font`]. Built-in widgets read text through
//! `Style.font_token`; layout and render look the token up at draw
//! time. The bundled provider is the 8x8 ASCII bitmap (the renderer
//! has always shipped); third-party providers (SDF / TTF) plug in
//! through [`FontProvider`] behind [`FontBackend::Custom`].
//!
//! Resource-management surface only: this module does not implement
//! glyph rasterization for non-bitmap providers. SDF / TTF rendering
//! lands as a separate milestone implementing `FontProvider`.

pub mod bitmap_8x8;

pub use bitmap_8x8::{CHAR_H, CHAR_W, FONT_8X8, glyph};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use crate::ecs::World;
use crate::resource::HasProbe;

/// One glyph as the framework consumes it.
///
/// The 8x8 bitmap renderer reads `bitmap` directly. SDF / TTF
/// providers extend the type via their own data; built-in widgets
/// only need `advance` for layout.
#[derive(Clone, Debug)]
pub struct Glyph {
    /// Horizontal advance after drawing this glyph, in pixels.
    pub advance: u16,
    /// Bitmap rows (one byte per row, MSB = leftmost pixel) for the
    /// 8x8 backend. Empty for backends that draw through their own
    /// path (custom providers ignore this field and read from their
    /// own state).
    pub bitmap: &'static [u8],
}

/// Cheap font metadata that layout reads without touching glyph data.
///
/// All values in pixels. `line_height` is the recommended vertical
/// advance between baselines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontMetrics {
    pub ascender: u16,
    pub descender: u16,
    pub line_height: u16,
}

/// Trait for swapping in a glyph provider (SDF, TTF, etc.).
///
/// `Bitmap8x8` is the built-in provider; sdf-fonts and other
/// milestones implement this trait and plug in via
/// [`FontBackend::Custom`].
pub trait FontProvider: 'static {
    fn glyph(&self, ch: char) -> Option<Glyph>;
    fn metrics(&self) -> FontMetrics;
}

/// Glyph source backing a [`Font`].
///
/// `Bitmap8x8` ships as the only built-in renderable provider; `Custom`
/// is the extension point.
pub enum FontBackend {
    /// The framework's bundled 8x8 ASCII bitmap.
    Bitmap8x8,
    /// User- or milestone-supplied provider.
    Custom(Box<dyn FontProvider>),
}

impl core::fmt::Debug for FontBackend {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FontBackend::Bitmap8x8 => f.write_str("Bitmap8x8"),
            FontBackend::Custom(_) => f.write_str("Custom(<dyn FontProvider>)"),
        }
    }
}

/// A font resource: family / size identity + a glyph backend.
#[derive(Debug)]
pub struct Font {
    pub family: &'static str,
    pub size: u16,
    pub backend: FontBackend,
}

impl Font {
    /// The bundled 8x8 ASCII bitmap font. This is the framework's
    /// default and the only renderable provider in the bare crate.
    pub fn bitmap_8x8() -> Self {
        Self {
            family: "bitmap8x8",
            size: 8,
            backend: FontBackend::Bitmap8x8,
        }
    }

    /// Resolve a single glyph through the active backend.
    pub fn glyph(&self, ch: char) -> Option<Glyph> {
        match &self.backend {
            FontBackend::Bitmap8x8 => bitmap_8x8_glyph(ch),
            FontBackend::Custom(p) => p.glyph(ch),
        }
    }

    /// Cheap metrics — no glyph touch.
    pub fn metrics(&self) -> FontMetrics {
        match &self.backend {
            FontBackend::Bitmap8x8 => BITMAP_8X8_METRICS,
            FontBackend::Custom(p) => p.metrics(),
        }
    }
}

const BITMAP_8X8_METRICS: FontMetrics = FontMetrics {
    ascender: 7,
    descender: 1,
    line_height: 8,
};

fn bitmap_8x8_glyph(ch: char) -> Option<Glyph> {
    // ASCII 32..127 only; outside range the legacy `glyph(u8)` fn
    // returns the '?' bitmap, which keeps the renderer always-drawing.
    let byte = if ('\u{20}'..'\u{7f}').contains(&ch) {
        ch as u8
    } else {
        b'?'
    };
    let bitmap: &'static [u8; 8] = bitmap_8x8::glyph(byte);
    Some(Glyph {
        advance: bitmap_8x8::CHAR_W as u16,
        bitmap,
    })
}

impl crate::cache::HasSize for FontMetrics {
    fn cache_size(&self) -> usize {
        core::mem::size_of::<Self>()
    }
}

impl HasProbe for Font {
    type Meta = FontMetrics;

    fn extract_meta(&self) -> Self::Meta {
        self.metrics()
    }
}

/// Identifier for a font slot in a [`FontRegistry`]. Widget styles
/// reference a token rather than a concrete `Font` so the visual
/// theme can swap fonts without touching widgets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontToken {
    Default,
    Heading,
    Mono,
    Custom(&'static str),
}

/// World resource: maps [`FontToken`] to [`Font`].
///
/// `Default` is always populated (falls back to [`Font::bitmap_8x8`]
/// when not overridden). Any other token resolves to `Default` if not
/// registered, so widgets never see a missing font.
pub struct FontRegistry {
    fonts: BTreeMap<FontToken, Font>,
}

impl FontRegistry {
    /// New registry with `Default` bound to the 8x8 bitmap font.
    pub fn new() -> Self {
        let mut fonts = BTreeMap::new();
        fonts.insert(FontToken::Default, Font::bitmap_8x8());
        Self { fonts }
    }

    /// Bind a font to a token.
    pub fn set(&mut self, token: FontToken, font: Font) -> &mut Self {
        self.fonts.insert(token, font);
        self
    }

    /// Look up a font, falling back to `Default` when the token is
    /// unbound.
    pub fn resolve(&self, token: &FontToken) -> &Font {
        self.fonts
            .get(token)
            .or_else(|| self.fonts.get(&FontToken::Default))
            .expect("FontRegistry::Default invariant")
    }

    /// Whether a token is explicitly bound (vs. falling through to
    /// `Default`).
    pub fn contains(&self, token: &FontToken) -> bool {
        self.fonts.contains_key(token)
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: resolve `token` against the World's `FontRegistry`,
/// installing a default registry if none is present.
pub fn resolve_or_default<'a>(world: &'a World, token: &FontToken) -> Option<&'a Font> {
    world.resource::<FontRegistry>().map(|r| r.resolve(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_token_resolves_to_bitmap8x8() {
        let reg = FontRegistry::new();
        let f = reg.resolve(&FontToken::Default);
        assert_eq!(f.family, "bitmap8x8");
        assert_eq!(f.size, 8);
    }

    #[test]
    fn unbound_token_falls_back_to_default() {
        let reg = FontRegistry::new();
        let f = reg.resolve(&FontToken::Heading);
        assert_eq!(f.family, "bitmap8x8");
        assert!(!reg.contains(&FontToken::Heading));
    }

    #[test]
    fn set_overrides_token_resolution() {
        let mut reg = FontRegistry::new();
        reg.set(
            FontToken::Heading,
            Font {
                family: "fake-heading",
                size: 16,
                backend: FontBackend::Bitmap8x8,
            },
        );
        let f = reg.resolve(&FontToken::Heading);
        assert_eq!(f.family, "fake-heading");
        assert_eq!(f.size, 16);
        assert!(reg.contains(&FontToken::Heading));
    }

    #[test]
    fn set_default_overrides_bundled_font() {
        let mut reg = FontRegistry::new();
        reg.set(
            FontToken::Default,
            Font {
                family: "user-default",
                size: 12,
                backend: FontBackend::Bitmap8x8,
            },
        );
        assert_eq!(reg.resolve(&FontToken::Default).family, "user-default");
    }

    #[test]
    fn custom_token_resolves_when_registered() {
        let mut reg = FontRegistry::new();
        let token = FontToken::Custom("brand");
        reg.set(
            token.clone(),
            Font {
                family: "brand-face",
                size: 10,
                backend: FontBackend::Bitmap8x8,
            },
        );
        assert_eq!(reg.resolve(&token).family, "brand-face");
    }

    #[test]
    fn metrics_is_extractable_via_has_probe() {
        let font = Font::bitmap_8x8();
        let meta = font.extract_meta();
        assert_eq!(meta, BITMAP_8X8_METRICS);
        assert_eq!(meta.line_height, 8);
    }

    #[test]
    fn glyph_roundtrip_for_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('A').expect("ASCII glyph");
        assert_eq!(g.advance, 8);
        assert_eq!(g.bitmap.len(), 8);
    }

    #[test]
    fn glyph_falls_back_to_question_mark_outside_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('日').expect("fallback glyph");
        let q = font.glyph('?').expect("? glyph");
        assert_eq!(g.bitmap, q.bitmap);
    }

    #[test]
    fn custom_provider_routes_through_backend() {
        struct AllX;
        impl FontProvider for AllX {
            fn glyph(&self, _ch: char) -> Option<Glyph> {
                Some(Glyph {
                    advance: 6,
                    bitmap: &[0xFF; 8],
                })
            }
            fn metrics(&self) -> FontMetrics {
                FontMetrics {
                    ascender: 6,
                    descender: 0,
                    line_height: 6,
                }
            }
        }
        let font = Font {
            family: "all-x",
            size: 6,
            backend: FontBackend::Custom(Box::new(AllX)),
        };
        assert_eq!(font.glyph('A').unwrap().advance, 6);
        assert_eq!(font.metrics().line_height, 6);
    }
}
