//! Font resource management — manager, token, provider trait.
//!
//! [`FontManager`] (a `ResourceManager<Font>`) is the World resource
//! that maps a [`FontToken`] cache key to a concrete [`Font`]. Built-in
//! widgets read text through `Style.font_token`; layout and render look
//! the token up at draw time. The fallback provider is the 8x8 ASCII
//! bitmap; third-party providers plug in through [`FontProvider`] behind
//! [`FontBackend::Custom`].

pub mod bitmap_8x8;
pub mod chunk;
pub mod sdf;

pub use bitmap_8x8::{CHAR_H, CHAR_W, FONT_8X8, glyph};

use alloc::rc::Rc;

use crate::core::resource::{HasProbe, ResourceManager};
use crate::ecs::World;

/// One glyph the renderer consumes: an `advance` for layout and a
/// [`GlyphKind`] payload that selects the rasterization scheme.
#[derive(Clone, Debug)]
pub struct Glyph {
    /// Horizontal advance after drawing this glyph, in pixels.
    pub advance: u16,
    pub kind: GlyphKind,
}

/// Rasterization scheme tag — renderers match on this to pick how to
/// draw the glyph. `non_exhaustive` so new variants stay non-breaking.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum GlyphKind {
    /// Bitmap rows, one byte per row, MSB = leftmost pixel.
    Mono(&'static [u8]),
    /// Signed-distance-field atlas slice. `atlas` holds packed
    /// distances at `bit_depth` per pixel, layout
    /// `source_size × source_size` row-major. The renderer scales to
    /// the requested target size by sampling.
    Sdf {
        atlas: &'static [u8],
        source_size: u16,
        bit_depth: u8,
        /// Pixels of zero-distance band encoded around the glyph edge
        /// (atlas-time choice). Required to convert the quantized
        /// distance back to source pixels.
        spread: u16,
        bbox_w: u8,
        bbox_h: u8,
        bearing_x: i8,
        bearing_y: i8,
    },
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

/// A glyph source. Implementors plug into [`FontBackend::Custom`] to
/// supply glyphs from any rasterization scheme (bitmap, SDF, TTF, …).
pub trait FontProvider: 'static {
    fn glyph(&self, ch: char) -> Option<Glyph>;
    fn metrics(&self) -> FontMetrics;
}

/// Glyph source backing a [`Font`]: the bundled 8x8 bitmap, or a
/// caller-supplied [`FontProvider`]. `Rc` (not `Box`) so `Font` is
/// `Clone` — `ResourceManager<Font>` requires it, and a clone is a
/// refcount bump, never a provider copy.
#[derive(Clone)]
pub enum FontBackend {
    /// 8x8 ASCII bitmap, ASCII 32..127.
    Bitmap8x8,
    /// Caller-supplied provider.
    Custom(Rc<dyn FontProvider>),
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
#[derive(Debug, Clone)]
pub struct Font {
    pub family: &'static str,
    pub size: u16,
    pub backend: FontBackend,
}

impl Font {
    pub fn bitmap_8x8() -> Self {
        Self {
            family: "bitmap8x8",
            size: 8,
            backend: FontBackend::Bitmap8x8,
        }
    }

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
    let byte = if ('\u{20}'..'\u{7f}').contains(&ch) {
        ch as u8
    } else {
        b'?'
    };
    let bitmap: &'static [u8; 8] = bitmap_8x8::glyph(byte);
    Some(Glyph {
        advance: bitmap_8x8::CHAR_W as u16,
        kind: GlyphKind::Mono(bitmap),
    })
}

impl crate::core::cache::HasSize for FontMetrics {
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

impl crate::core::cache::HasSize for Font {
    // Glyph atlases live in flash via &'static slices, not the heap, so
    // the owned struct size (not the atlas bytes) is the right LRU
    // weight.
    fn cache_size(&self) -> usize {
        core::mem::size_of::<Self>()
    }
}

/// Identifier for a font slot, parallel to `ColorToken`. Widget styles
/// reference a token rather than a concrete `Font` so the theme can
/// swap fonts without touching widgets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontToken {
    Default,
    Heading,
    Mono,
    Custom(&'static str),
}

impl FontToken {
    fn family_key(&self) -> &str {
        match self {
            FontToken::Default => "default",
            FontToken::Heading => "heading",
            FontToken::Mono => "mono",
            FontToken::Custom(s) => s,
        }
    }

    /// Serialize to the `FontManager` cache key. Centralized here so
    /// layout and render always produce the same string for one logical
    /// font — hand-formatting at call sites would split a font across
    /// two cache entries.
    pub fn cache_key(&self) -> alloc::borrow::Cow<'static, str> {
        alloc::borrow::Cow::Owned(alloc::format!("font:{}", self.family_key()))
    }
}

/// World resource mapping a [`FontToken`] cache key to a [`Font`].
///
/// The manager's fallback is [`Font::bitmap_8x8`], so any unregistered
/// token resolves to the bundled bitmap — widgets never see a missing
/// font.
pub type FontManager = ResourceManager<Font>;

/// Build the default font manager: an unbounded-budget
/// [`ResourceManager`] whose fallback is the 8x8 bitmap.
pub fn default_font_manager() -> FontManager {
    ResourceManager::new(crate::core::cache::MaxSize::Unbound, Font::bitmap_8x8())
}

/// Resolve `token` against the World's [`FontManager`], or `None` when
/// the manager has not been inserted yet. Returns an owned `Rc<Font>`
/// so the `&World` borrow ends at the call — the render path holds the
/// `Rc` locally instead of borrowing through the manager's `RefCell`.
pub fn resolve_or_default(world: &World, token: &FontToken) -> Option<Rc<Font>> {
    world
        .resource::<FontManager>()
        .map(|m| m.resolve(&token.cache_key()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_token_resolves_to_bitmap8x8() {
        let mgr = default_font_manager();
        let f = mgr.resolve(&FontToken::Default.cache_key());
        assert_eq!(f.family, "bitmap8x8");
        assert_eq!(f.size, 8);
    }

    #[test]
    fn unbound_token_falls_back_to_bitmap() {
        let mgr = default_font_manager();
        let f = mgr.resolve(&FontToken::Heading.cache_key());
        assert_eq!(f.family, "bitmap8x8");
    }

    #[test]
    fn registered_token_overrides_fallback() {
        let mgr = default_font_manager();
        mgr.add_static(
            FontToken::Heading.cache_key(),
            Font {
                family: "fake-heading",
                size: 16,
                backend: FontBackend::Bitmap8x8,
            },
        );
        let f = mgr.resolve(&FontToken::Heading.cache_key());
        assert_eq!(f.family, "fake-heading");
        assert_eq!(f.size, 16);
    }

    #[test]
    fn registering_default_key_overrides_bundled_font() {
        let mgr = default_font_manager();
        mgr.add_static(
            FontToken::Default.cache_key(),
            Font {
                family: "user-default",
                size: 12,
                backend: FontBackend::Bitmap8x8,
            },
        );
        assert_eq!(
            mgr.resolve(&FontToken::Default.cache_key()).family,
            "user-default"
        );
    }

    #[test]
    fn custom_token_resolves_when_registered() {
        let mgr = default_font_manager();
        let token = FontToken::Custom("brand");
        mgr.add_static(
            token.cache_key(),
            Font {
                family: "brand-face",
                size: 10,
                backend: FontBackend::Bitmap8x8,
            },
        );
        assert_eq!(mgr.resolve(&token.cache_key()).family, "brand-face");
    }

    #[test]
    fn cache_key_is_stable_per_token() {
        assert_eq!(FontToken::Default.cache_key(), "font:default");
        assert_eq!(FontToken::Heading.cache_key(), "font:heading");
        assert_eq!(FontToken::Custom("brand").cache_key(), "font:brand");
    }

    #[test]
    fn metrics_is_extractable_via_has_probe() {
        let font = Font::bitmap_8x8();
        let meta = font.extract_meta();
        assert_eq!(meta, BITMAP_8X8_METRICS);
        assert_eq!(meta.line_height, 8);
    }

    fn unwrap_mono(g: &Glyph) -> &[u8] {
        match &g.kind {
            GlyphKind::Mono(b) => b,
            other => panic!("expected Mono, got {:?}", other),
        }
    }

    #[test]
    fn glyph_roundtrip_for_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('A').expect("ASCII glyph");
        assert_eq!(g.advance, 8);
        assert_eq!(unwrap_mono(&g).len(), 8);
    }

    #[test]
    fn glyph_falls_back_to_question_mark_outside_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('日').expect("fallback glyph");
        let q = font.glyph('?').expect("? glyph");
        assert_eq!(unwrap_mono(&g), unwrap_mono(&q));
    }

    #[test]
    fn glyph_returns_mono_for_bitmap_8x8() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('A').expect("ASCII glyph");
        assert!(matches!(g.kind, GlyphKind::Mono(_)));
    }

    #[test]
    fn custom_provider_routes_through_backend() {
        struct AllX;
        impl FontProvider for AllX {
            fn glyph(&self, _ch: char) -> Option<Glyph> {
                Some(Glyph {
                    advance: 6,
                    kind: GlyphKind::Mono(&[0xFF; 8]),
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
            backend: FontBackend::Custom(Rc::new(AllX)),
        };
        assert_eq!(font.glyph('A').unwrap().advance, 6);
        assert_eq!(font.metrics().line_height, 6);
    }
}
