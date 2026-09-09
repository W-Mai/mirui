//! Font resources, glyph identity and raster access.
//!
//! [`FontManager`] (a `ResourceManager<Font>`) is the World resource
//! that maps a [`FontToken`] cache key to a concrete [`Font`]. Built-in
//! widgets resolve a [`FontStack`] through the manager. The fallback provider
//! is the 8x8 ASCII bitmap; external font formats plug in through
//! [`FontProvider`] behind [`FontBackend::Custom`].

pub mod bitmap_8x8;
pub mod mirx;
pub mod sdf;

pub use bitmap_8x8::{CHAR_H, CHAR_W, FONT_8X8, glyph};

use alloc::{borrow::Cow, rc::Rc};

use crate::core::resource::{HasProbe, ResourceManager};
use crate::ecs::World;
use crate::types::{Fixed, fixed::to_textflow};

pub use textflow::shaping::{FontId as FontFaceId, GlyphId};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FontSurfaceId(u64);

impl FontSurfaceId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphSurfaceError {
    UnsupportedLayout,
    StrideTooSmall { minimum: u32, actual: u32 },
    SizeOverflow,
    DataTooSmall { needed: usize, available: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphSurface<'a> {
    samples: &'a [u8],
    width: u32,
    height: u32,
    stride: u32,
    sample_layout: ::mirx::image::SampleLayout,
    alignment: ::mirx::types::ByteAlignment,
    id: FontSurfaceId,
}

impl<'a> GlyphSurface<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        samples: &'a [u8],
        width: u32,
        height: u32,
        stride: u32,
        sample_layout: ::mirx::image::SampleLayout,
        alignment: ::mirx::types::ByteAlignment,
        id: FontSurfaceId,
    ) -> Result<Self, GlyphSurfaceError> {
        let geometry = sample_layout
            .plane_geometry(width, height, 0)
            .filter(|_| sample_layout.is_alpha())
            .ok_or(GlyphSurfaceError::UnsupportedLayout)?;
        let minimum = geometry
            .minimum_stride()
            .ok_or(GlyphSurfaceError::SizeOverflow)?;
        if stride < minimum {
            return Err(GlyphSurfaceError::StrideTooSmall {
                minimum,
                actual: stride,
            });
        }
        let needed = usize::try_from(stride)
            .ok()
            .and_then(|value| value.checked_mul(height as usize))
            .ok_or(GlyphSurfaceError::SizeOverflow)?;
        if samples.len() < needed {
            return Err(GlyphSurfaceError::DataTooSmall {
                needed,
                available: samples.len(),
            });
        }
        Ok(Self {
            samples,
            width,
            height,
            stride,
            sample_layout,
            alignment,
            id,
        })
    }

    pub const fn samples(self) -> &'a [u8] {
        self.samples
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn stride(self) -> u32 {
        self.stride
    }

    pub const fn sample_layout(self) -> ::mirx::image::SampleLayout {
        self.sample_layout
    }

    pub const fn alignment(self) -> ::mirx::types::ByteAlignment {
        self.alignment
    }

    pub const fn id(self) -> FontSurfaceId {
        self.id
    }

    pub fn address_is_aligned(self) -> bool {
        self.samples.is_empty()
            || self.samples.as_ptr() as usize % self.alignment.get() as usize == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RasterGlyph<'a> {
    pub surface: GlyphSurface<'a>,
    pub region: Option<::mirx::image::Region>,
    pub offset_x: Fixed,
    pub offset_y: Fixed,
    pub representation: ::mirx::font::FontRepresentation,
}

/// One glyph the renderer consumes: an `advance` for layout and a
/// [`GlyphKind`] payload that selects the rasterization scheme.
#[derive(Clone, Debug)]
pub struct Glyph<'a> {
    /// Horizontal advance in logical pixels at the requested size.
    pub advance: crate::types::Fixed,
    pub kind: GlyphKind<'a>,
}

/// Rasterization scheme tag — renderers match on this to pick how to
/// draw the glyph. `non_exhaustive` so new variants stay non-breaking.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum GlyphKind<'a> {
    /// Bitmap rows, one byte per row, MSB = leftmost pixel.
    Mono(&'static [u8]),
    /// Borrowed scalar samples in their declared physical plane.
    Raster {
        samples: &'a [u8],
        stride: u32,
        region: ::mirx::image::Region,
        representation: ::mirx::font::FontRepresentation,
        /// Horizontal offset in logical pixels at the requested size.
        bearing_x: crate::types::Fixed,
        /// Baseline offset in logical pixels at the requested size.
        bearing_y: crate::types::Fixed,
    },
}

/// Cheap font metadata that layout reads without touching glyph data.
///
/// All values in pixels. `line_height` is the recommended vertical
/// advance between baselines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontMetrics {
    pub ascender: crate::types::Fixed,
    pub descender: crate::types::Fixed,
    pub line_height: crate::types::Fixed,
}

/// Format adapter used by both paragraph layout and render backends.
pub trait FontProvider: 'static {
    fn face_id(&self) -> FontFaceId;
    fn map_char(&self, ch: char) -> Option<GlyphId>;
    fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<Fixed>;
    fn raster(&self, glyph: GlyphId, ppem: u16) -> Option<RasterGlyph<'_>>;
    fn metrics(&self, ppem: u16) -> FontMetrics;

    fn notdef_glyph(&self) -> Option<GlyphId> {
        Some(GlyphId::new(0))
    }

    fn pair_kerning(&self, _left: GlyphId, _right: GlyphId, _ppem: u16) -> Fixed {
        Fixed::ZERO
    }

    fn shaping_data(&self) -> Option<&[u8]> {
        None
    }

    fn covers(&self, cluster: &str) -> bool {
        !cluster.is_empty() && cluster.chars().all(|ch| self.map_char(ch).is_some())
    }

    fn shape_into(
        &self,
        ppem: u16,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        let source = ProviderGlyphSource {
            provider: self,
            ppem,
        };
        textflow::shaping::Typeface::shape_into(
            &textflow::shaping::SimpleTypeface::new(&source),
            request,
            output,
        )
    }
}

struct ProviderGlyphSource<'a, P: FontProvider + ?Sized> {
    provider: &'a P,
    ppem: u16,
}

impl<P: FontProvider + ?Sized> textflow::shaping::GlyphSource for ProviderGlyphSource<'_, P> {
    fn id(&self) -> FontFaceId {
        self.provider.face_id()
    }

    fn metrics(
        &self,
    ) -> Result<textflow::shaping::FontMetrics, textflow::shaping::FontAccessError> {
        let metrics = self.provider.metrics(self.ppem);
        Ok(textflow::shaping::FontMetrics {
            units_per_em: self.ppem.max(1),
            ascender: to_textflow(metrics.ascender),
            descender: to_textflow(metrics.descender),
            line_gap: to_textflow(metrics.line_height - metrics.ascender + metrics.descender),
        })
    }

    fn glyph_for(
        &self,
        character: char,
    ) -> Result<Option<GlyphId>, textflow::shaping::FontAccessError> {
        Ok(self.provider.map_char(character))
    }

    fn glyph_advance(
        &self,
        glyph: GlyphId,
    ) -> Result<textflow::shaping::FlowPoint, textflow::shaping::FontAccessError> {
        let x = self
            .provider
            .glyph_advance(glyph, self.ppem)
            .ok_or(textflow::shaping::FontAccessError::Malformed)?;
        Ok(textflow::shaping::FlowPoint {
            x: to_textflow(x),
            y: 0,
        })
    }

    fn notdef_glyph(&self) -> Result<Option<GlyphId>, textflow::shaping::FontAccessError> {
        Ok(self.provider.notdef_glyph())
    }

    fn kerning(
        &self,
        left: GlyphId,
        right: GlyphId,
    ) -> Result<i32, textflow::shaping::FontAccessError> {
        Ok(to_textflow(
            self.provider.pair_kerning(left, right, self.ppem),
        ))
    }
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

    pub fn face_id(&self) -> FontFaceId {
        match &self.backend {
            FontBackend::Bitmap8x8 => FontFaceId::new(1),
            FontBackend::Custom(provider) => provider.face_id(),
        }
    }

    pub fn map_char(&self, ch: char) -> Option<GlyphId> {
        match &self.backend {
            FontBackend::Bitmap8x8 => ('\u{20}'..'\u{7f}')
                .contains(&ch)
                .then_some(GlyphId::new(ch as u16)),
            FontBackend::Custom(provider) => provider.map_char(ch),
        }
    }

    pub fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<Fixed> {
        match &self.backend {
            FontBackend::Bitmap8x8 => Some(Fixed::from_int(bitmap_8x8::CHAR_W as i32)),
            FontBackend::Custom(provider) => provider.glyph_advance(glyph, ppem),
        }
    }

    pub fn raster(&self, glyph: GlyphId, ppem: u16) -> Option<RasterGlyph<'_>> {
        match &self.backend {
            FontBackend::Bitmap8x8 => None,
            FontBackend::Custom(provider) => provider.raster(glyph, ppem),
        }
    }

    pub fn glyph(&self, ch: char, requested_size: u16) -> Option<Glyph<'_>> {
        match &self.backend {
            FontBackend::Bitmap8x8 => bitmap_8x8_glyph(ch),
            FontBackend::Custom(provider) => {
                let glyph = provider.map_char(ch).or_else(|| provider.notdef_glyph())?;
                let advance = provider.glyph_advance(glyph, requested_size)?;
                let raster = provider.raster(glyph, requested_size)?;
                let region = raster.region.unwrap_or_else(|| {
                    ::mirx::image::Region::new(0, 0, 0, 0).expect("empty glyph region")
                });
                Some(Glyph {
                    advance,
                    kind: GlyphKind::Raster {
                        samples: raster.surface.samples,
                        stride: raster.surface.stride,
                        region,
                        representation: raster.representation,
                        bearing_x: raster.offset_x,
                        bearing_y: raster.offset_y,
                    },
                })
            }
        }
    }

    /// Cheap metrics — no glyph touch.
    pub fn metrics(&self, requested_size: u16) -> FontMetrics {
        match &self.backend {
            FontBackend::Bitmap8x8 => BITMAP_8X8_METRICS,
            FontBackend::Custom(p) => p.metrics(requested_size),
        }
    }

    pub fn covers(&self, cluster: &str) -> bool {
        match &self.backend {
            FontBackend::Bitmap8x8 => {
                !cluster.is_empty() && cluster.chars().all(|ch| ('\u{20}'..'\u{7f}').contains(&ch))
            }
            FontBackend::Custom(provider) => provider.covers(cluster),
        }
    }

    pub fn shape_into(
        &self,
        ppem: u16,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        match &self.backend {
            FontBackend::Bitmap8x8 => {
                let source = FontGlyphSource { font: self, ppem };
                textflow::shaping::Typeface::shape_into(
                    &textflow::shaping::SimpleTypeface::new(&source),
                    request,
                    output,
                )
            }
            FontBackend::Custom(provider) => provider.shape_into(ppem, request, output),
        }
    }
}

struct FontGlyphSource<'a> {
    font: &'a Font,
    ppem: u16,
}

impl textflow::shaping::GlyphSource for FontGlyphSource<'_> {
    fn id(&self) -> FontFaceId {
        self.font.face_id()
    }

    fn metrics(
        &self,
    ) -> Result<textflow::shaping::FontMetrics, textflow::shaping::FontAccessError> {
        let metrics = self.font.metrics(self.ppem);
        Ok(textflow::shaping::FontMetrics {
            units_per_em: self.ppem.max(1),
            ascender: to_textflow(metrics.ascender),
            descender: to_textflow(metrics.descender),
            line_gap: to_textflow(metrics.line_height - metrics.ascender + metrics.descender),
        })
    }

    fn glyph_for(
        &self,
        character: char,
    ) -> Result<Option<GlyphId>, textflow::shaping::FontAccessError> {
        Ok(self.font.map_char(character))
    }

    fn glyph_advance(
        &self,
        glyph: GlyphId,
    ) -> Result<textflow::shaping::FlowPoint, textflow::shaping::FontAccessError> {
        let advance = self
            .font
            .glyph_advance(glyph, self.ppem)
            .ok_or(textflow::shaping::FontAccessError::Malformed)?;
        Ok(textflow::shaping::FlowPoint {
            x: to_textflow(advance),
            y: 0,
        })
    }
}

pub struct FontTypeface<'a> {
    font: &'a Font,
    ppem: u16,
}

impl<'a> FontTypeface<'a> {
    pub const fn new(font: &'a Font, ppem: u16) -> Self {
        Self { font, ppem }
    }
}

impl textflow::shaping::Typeface for FontTypeface<'_> {
    fn id(&self) -> FontFaceId {
        self.font.face_id()
    }

    fn metrics(
        &self,
    ) -> Result<textflow::shaping::FontMetrics, textflow::shaping::FontAccessError> {
        textflow::shaping::GlyphSource::metrics(&FontGlyphSource {
            font: self.font,
            ppem: self.ppem,
        })
    }

    fn covers(&self, cluster: &str) -> Result<bool, textflow::shaping::FontAccessError> {
        Ok(self.font.covers(cluster))
    }

    fn shape_into(
        &self,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        self.font.shape_into(self.ppem, request, output)
    }
}

const BITMAP_8X8_METRICS: FontMetrics = FontMetrics {
    ascender: crate::types::Fixed::from_int(7),
    descender: crate::types::Fixed::from_int(-1),
    line_height: crate::types::Fixed::from_int(8),
};

fn bitmap_8x8_glyph(ch: char) -> Option<Glyph<'static>> {
    let byte = if ('\u{20}'..'\u{7f}').contains(&ch) {
        ch as u8
    } else {
        b'?'
    };
    let bitmap: &'static [u8; 8] = bitmap_8x8::glyph(byte);
    Some(Glyph {
        advance: crate::types::Fixed::from_int(bitmap_8x8::CHAR_W as i32),
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
        self.metrics(self.size)
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FontStack {
    primary: FontToken,
    fallbacks: Cow<'static, [FontToken]>,
}

impl FontStack {
    pub const fn new(primary: FontToken) -> Self {
        Self {
            primary,
            fallbacks: Cow::Borrowed(&[]),
        }
    }

    pub fn with_fallbacks(mut self, fallbacks: impl Into<Cow<'static, [FontToken]>>) -> Self {
        self.fallbacks = fallbacks.into();
        self
    }

    pub const fn primary(&self) -> &FontToken {
        &self.primary
    }

    pub fn fallbacks(&self) -> &[FontToken] {
        &self.fallbacks
    }

    pub fn iter(&self) -> impl Iterator<Item = &FontToken> {
        core::iter::once(&self.primary).chain(self.fallbacks.iter())
    }
}

impl Default for FontStack {
    fn default() -> Self {
        Self::new(FontToken::Default)
    }
}

impl From<FontToken> for FontStack {
    fn from(primary: FontToken) -> Self {
        Self::new(primary)
    }
}

impl FontToken {
    pub const fn cache_key(&self) -> &'static str {
        match self {
            FontToken::Default => "\0mirui-font-default",
            FontToken::Heading => "\0mirui-font-heading",
            FontToken::Mono => "\0mirui-font-mono",
            FontToken::Custom(value) => value,
        }
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
        .map(|m| m.resolve(token.cache_key()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_token_resolves_to_bitmap8x8() {
        let mgr = default_font_manager();
        let f = mgr.resolve(FontToken::Default.cache_key());
        assert_eq!(f.family, "bitmap8x8");
        assert_eq!(f.size, 8);
    }

    #[test]
    fn unbound_token_falls_back_to_bitmap() {
        let mgr = default_font_manager();
        let f = mgr.resolve(FontToken::Heading.cache_key());
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
        let f = mgr.resolve(FontToken::Heading.cache_key());
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
            mgr.resolve(FontToken::Default.cache_key()).family,
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
        assert_eq!(mgr.resolve(token.cache_key()).family, "brand-face");
    }

    #[test]
    fn cache_key_is_stable_per_token() {
        assert_eq!(FontToken::Default.cache_key(), "\0mirui-font-default");
        assert_eq!(FontToken::Heading.cache_key(), "\0mirui-font-heading");
        assert_eq!(FontToken::Custom("brand").cache_key(), "brand");
    }

    #[test]
    fn cache_key_never_allocates() {
        assert_eq!(
            FontToken::Custom("brand").cache_key().as_ptr(),
            "brand".as_ptr()
        );
    }

    #[test]
    fn font_stack_keeps_borrowed_fallback_order() {
        static FALLBACKS: [FontToken; 2] = [FontToken::Mono, FontToken::Custom("symbols")];
        let stack = FontStack::new(FontToken::Heading).with_fallbacks(&FALLBACKS[..]);
        assert_eq!(
            stack.iter().collect::<alloc::vec::Vec<_>>(),
            alloc::vec![
                &FontToken::Heading,
                &FontToken::Mono,
                &FontToken::Custom("symbols")
            ]
        );
        assert!(matches!(stack.fallbacks, Cow::Borrowed(_)));
    }

    #[test]
    fn glyph_surface_rejects_short_rows_and_storage() {
        let id = FontSurfaceId::new(7);
        assert!(matches!(
            GlyphSurface::new(
                &[0; 1],
                9,
                1,
                1,
                ::mirx::image::SampleLayout::A1,
                ::mirx::types::ByteAlignment::ONE,
                id,
            ),
            Err(GlyphSurfaceError::StrideTooSmall {
                minimum: 2,
                actual: 1
            })
        ));
        assert!(matches!(
            GlyphSurface::new(
                &[0; 1],
                8,
                2,
                1,
                ::mirx::image::SampleLayout::A1,
                ::mirx::types::ByteAlignment::ONE,
                id,
            ),
            Err(GlyphSurfaceError::DataTooSmall {
                needed: 2,
                available: 1
            })
        ));
    }

    #[test]
    fn glyph_surface_reports_actual_address_alignment() {
        #[repr(align(64))]
        struct Aligned([u8; 65]);

        let bytes = Aligned([0; 65]);
        let aligned = GlyphSurface::new(
            &bytes.0[..1],
            8,
            1,
            1,
            ::mirx::image::SampleLayout::A1,
            ::mirx::types::ByteAlignment::new(64).unwrap(),
            FontSurfaceId::new(7),
        )
        .unwrap();
        let unaligned = GlyphSurface::new(
            &bytes.0[1..2],
            8,
            1,
            1,
            ::mirx::image::SampleLayout::A1,
            ::mirx::types::ByteAlignment::new(64).unwrap(),
            FontSurfaceId::new(8),
        )
        .unwrap();
        assert!(aligned.address_is_aligned());
        assert!(!unaligned.address_is_aligned());
    }

    #[test]
    fn metrics_is_extractable_via_has_probe() {
        let font = Font::bitmap_8x8();
        let meta = font.extract_meta();
        assert_eq!(meta, BITMAP_8X8_METRICS);
        assert_eq!(meta.line_height, crate::types::Fixed::from_int(8));
    }

    fn unwrap_mono<'a>(g: &'a Glyph<'a>) -> &'a [u8] {
        match &g.kind {
            GlyphKind::Mono(b) => b,
            other => panic!("expected Mono, got {:?}", other),
        }
    }

    #[test]
    fn glyph_roundtrip_for_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('A', 16).expect("ASCII glyph");
        assert_eq!(g.advance, crate::types::Fixed::from_int(8));
        assert_eq!(unwrap_mono(&g).len(), 8);
    }

    #[test]
    fn glyph_falls_back_to_question_mark_outside_ascii() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('日', 16).expect("fallback glyph");
        let q = font.glyph('?', 16).expect("? glyph");
        assert_eq!(unwrap_mono(&g), unwrap_mono(&q));
    }

    #[test]
    fn glyph_returns_mono_for_bitmap_8x8() {
        let font = Font::bitmap_8x8();
        let g = font.glyph('A', 16).expect("ASCII glyph");
        assert!(matches!(g.kind, GlyphKind::Mono(_)));
    }

    #[test]
    fn custom_provider_routes_through_backend() {
        struct AllX;
        impl FontProvider for AllX {
            fn face_id(&self) -> FontFaceId {
                FontFaceId::new(2)
            }
            fn map_char(&self, _ch: char) -> Option<GlyphId> {
                Some(GlyphId::new(1))
            }
            fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
                Some(Fixed::from_int(6))
            }
            fn raster(&self, _glyph: GlyphId, _ppem: u16) -> Option<RasterGlyph<'_>> {
                Some(RasterGlyph {
                    surface: GlyphSurface::new(
                        &[],
                        0,
                        0,
                        0,
                        ::mirx::image::SampleLayout::A1,
                        ::mirx::types::ByteAlignment::ONE,
                        FontSurfaceId::new(2),
                    )
                    .unwrap(),
                    region: None,
                    offset_x: Fixed::ZERO,
                    offset_y: Fixed::ZERO,
                    representation: ::mirx::font::FontRepresentation::coverage(1, 6, 0).unwrap(),
                })
            }
            fn metrics(&self, _requested_size: u16) -> FontMetrics {
                FontMetrics {
                    ascender: crate::types::Fixed::from_int(6),
                    descender: crate::types::Fixed::ZERO,
                    line_height: crate::types::Fixed::from_int(6),
                }
            }
        }
        let font = Font {
            family: "all-x",
            size: 6,
            backend: FontBackend::Custom(Rc::new(AllX)),
        };
        assert_eq!(
            font.glyph('A', 16).unwrap().advance,
            crate::types::Fixed::from_int(6)
        );
        assert_eq!(
            font.metrics(font.size).line_height,
            crate::types::Fixed::from_int(6)
        );

        let mut output = [textflow::shaping::ShapedGlyph::default(); 1];
        let request = textflow::shaping::ShapeRequest::new(
            "A",
            0..1,
            textflow::bidi::Direction::LeftToRight,
            textflow::unicode::Script::Latin,
        );
        assert_eq!(font.shape_into(6, &request, &mut output), Ok(1));
        assert_eq!(output[0].glyph_id(), GlyphId::new(1));
        assert_eq!(output[0].advance.x, to_textflow(Fixed::from_int(6)));
    }
}
