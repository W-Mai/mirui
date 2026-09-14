//! Font resources, glyph identity and raster access.
//!
//! [`FontManager`] (a `ResourceManager<Font>`) is the World resource
//! that maps a [`FontToken`] cache key to a concrete [`Font`]. Built-in
//! widgets resolve a [`FontStack`] through the manager. The fallback provider
//! is the 8x8 ASCII bitmap; external font formats plug in through
//! [`FontProvider`] behind [`FontBackend::Custom`].

pub mod bitmap_8x8;
pub mod mirx;
pub(crate) mod scalar;
pub mod sdf;

pub use bitmap_8x8::{CHAR_H, CHAR_W, FONT_8X8, glyph};

use alloc::{borrow::Cow, rc::Rc};

use crate::core::resource::{HasProbe, ResourceManager};
use crate::ecs::World;
use crate::types::{Fixed, Point, Rect, Transform, fixed::to_textflow};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RasterQuad {
    pub rect: Rect,
    pub transform: Transform,
}

impl RasterGlyph<'_> {
    pub(crate) fn posed_quad(
        self,
        pos: Point,
        frame: textflow::placement::GlyphFrame,
        requested_size: u16,
        transform: Transform,
    ) -> Option<RasterQuad> {
        let region = self
            .region
            .filter(|region| region.width() > 0 && region.height() > 0)?;
        let tangent = Point {
            x: crate::types::fixed::from_textflow(frame.unit_tangent.x),
            y: crate::types::fixed::from_textflow(frame.unit_tangent.y),
        };
        let origin = Point {
            x: pos.x + crate::types::fixed::from_textflow(frame.local_origin.x),
            y: pos.y + crate::types::fixed::from_textflow(frame.local_origin.y),
        };
        let pose = Transform {
            m00: tangent.x,
            m01: Fixed::ZERO - tangent.y,
            tx: origin.x,
            m10: tangent.y,
            m11: tangent.x,
            ty: origin.y,
        };
        let scale = Fixed::from_int(i32::from(requested_size))
            / Fixed::from_int(i32::from(self.representation.design_ppem().max(1)));
        Some(RasterQuad {
            rect: Rect {
                x: self.offset_x,
                y: Fixed::ZERO - self.offset_y,
                w: Fixed::from_int(i32::try_from(region.width()).ok()?) * scale,
                h: Fixed::from_int(i32::try_from(region.height()).ok()?) * scale,
            },
            transform: transform.compose(&pose),
        })
    }
}

/// One glyph the renderer consumes: an `advance` for layout and a
/// [`GlyphKind`] payload that selects the rasterization scheme.
#[derive(Clone, Debug)]
pub struct Glyph<'a> {
    /// Horizontal advance in logical pixels at the requested size.
    pub advance: crate::types::Fixed,
    pub kind: GlyphKind<'a>,
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RasterRunKey {
    glyph_hash: u64,
    face_id: FontFaceId,
    revision: u64,
    ppem: u16,
    color_rgb: u32,
    scale: Fixed,
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
impl RasterRunKey {
    fn new(
        font: &Font,
        glyphs: &[textflow::shaping::PositionedGlyph],
        color: &crate::types::Color,
        scale: Fixed,
    ) -> Self {
        let mut glyph_hash = 0xcbf2_9ce4_8422_2325;
        for glyph in glyphs {
            Self::extend(&mut glyph_hash, &glyph.glyph_id().value().to_le_bytes());
            Self::extend(&mut glyph_hash, &glyph.origin.x.to_le_bytes());
            Self::extend(&mut glyph_hash, &glyph.origin.y.to_le_bytes());
            Self::extend(&mut glyph_hash, &glyph.offset.x.to_le_bytes());
            Self::extend(&mut glyph_hash, &glyph.offset.y.to_le_bytes());
        }
        Self {
            glyph_hash,
            face_id: font.face_id(),
            revision: font.revision(),
            ppem: font.size,
            color_rgb: u32::from_be_bytes([color.r, color.g, color.b, 0]),
            scale,
        }
    }

    fn extend(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RasterRunBounds {
    pub offset: Point,
    pub size: Point,
    pub width: u16,
    pub height: u16,
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
impl RasterRunBounds {
    fn from_logical(bounds: crate::types::Rect, scale: Fixed) -> Option<Self> {
        if scale <= Fixed::ZERO {
            return None;
        }
        let physical = crate::types::Rect {
            x: bounds.x * scale,
            y: bounds.y * scale,
            w: bounds.w * scale,
            h: bounds.h * scale,
        };
        let (x0, y0, x1, y1) = physical.pixel_bounds();
        let width = u16::try_from(x1.checked_sub(x0)?)
            .ok()
            .filter(|value| *value > 0)?;
        let height = u16::try_from(y1.checked_sub(y0)?)
            .ok()
            .filter(|value| *value > 0)?;
        Some(Self {
            offset: Point {
                x: Fixed::from_int(x0) / scale,
                y: Fixed::from_int(y0) / scale,
            },
            size: Point {
                x: Fixed::from_int(i32::from(width)) / scale,
                y: Fixed::from_int(i32::from(height)) / scale,
            },
            width,
            height,
        })
    }

    fn new(
        font: &Font,
        glyphs: &[textflow::shaping::PositionedGlyph],
        output_ppem: u16,
        scale: Fixed,
    ) -> Option<Self> {
        if scale <= Fixed::ZERO {
            return None;
        }
        Self::from_logical(font.glyph_run_local_bounds(glyphs, output_ppem)?, scale)
    }
}

pub(crate) fn scaled_glyph_raster_extent(extent: u16, scale: Fixed) -> Option<u16> {
    let pixels = (Fixed::from(extent) * scale).ceil().to_int();
    u16::try_from(pixels).ok().filter(|value| *value > 0)
}

#[inline]
pub(crate) fn output_ppem(ppem: u16, scale: Fixed) -> u16 {
    scaled_glyph_raster_extent(ppem.max(1), scale).unwrap_or(u16::MAX)
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
    fn revision(&self) -> u64 {
        0
    }
    fn map_char(&self, ch: char) -> Option<GlyphId>;
    fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<Fixed>;
    /// Returns raster storage selected for `output_ppem`, with placement
    /// expressed in logical pixels at `layout_ppem`.
    fn raster(&self, glyph: GlyphId, layout_ppem: u16, output_ppem: u16)
    -> Option<RasterGlyph<'_>>;
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

    fn supports_complex_shaping(&self) -> bool {
        self.shaping_data().is_some()
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

    pub fn revision(&self) -> u64 {
        match &self.backend {
            FontBackend::Bitmap8x8 => 0,
            FontBackend::Custom(provider) => provider.revision(),
        }
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32")
    ))]
    pub(crate) fn raster_run_key(
        &self,
        glyphs: &[textflow::shaping::PositionedGlyph],
        color: &crate::types::Color,
        scale: Fixed,
    ) -> RasterRunKey {
        RasterRunKey::new(self, glyphs, color, scale)
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32")
    ))]
    pub(crate) fn raster_run_bounds(
        &self,
        glyphs: &[textflow::shaping::PositionedGlyph],
        output_ppem: u16,
        scale: Fixed,
    ) -> Option<RasterRunBounds> {
        RasterRunBounds::new(self, glyphs, output_ppem, scale)
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
            FontBackend::Custom(provider) => provider.raster(glyph, ppem, ppem),
        }
    }

    pub(crate) fn raster_for_output(
        &self,
        glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        match &self.backend {
            FontBackend::Bitmap8x8 => {
                let value = u8::try_from(glyph.value()).ok()?;
                let ordinal = value.checked_sub(b' ')?;
                if ordinal >= 95 {
                    return None;
                }
                Some(RasterGlyph {
                    surface: GlyphSurface::new(
                        bitmap_8x8::FONT_8X8,
                        bitmap_8x8::CHAR_W,
                        bitmap_8x8::CHAR_H * 95,
                        1,
                        ::mirx::image::SampleLayout::A1,
                        ::mirx::types::ByteAlignment::ONE,
                        FontSurfaceId::new(u64::MAX),
                    )
                    .ok()?,
                    region: ::mirx::image::Region::new(
                        0,
                        u32::from(ordinal) * bitmap_8x8::CHAR_H,
                        bitmap_8x8::CHAR_W,
                        bitmap_8x8::CHAR_H,
                    )
                    .ok(),
                    offset_x: Fixed::ZERO,
                    offset_y: BITMAP_8X8_METRICS.ascender,
                    representation: ::mirx::font::FontRepresentation::coverage(
                        1,
                        layout_ppem.max(1),
                        bitmap_8x8::FONT_8X8.len() as u32,
                    )
                    .ok()?,
                })
            }
            FontBackend::Custom(provider) => provider.raster(glyph, layout_ppem, output_ppem),
        }
    }

    pub fn glyph(&self, ch: char, requested_size: u16) -> Option<Glyph<'_>> {
        match &self.backend {
            FontBackend::Bitmap8x8 => bitmap_8x8_glyph(ch),
            FontBackend::Custom(provider) => {
                let glyph = provider.map_char(ch).or_else(|| provider.notdef_glyph())?;
                self.glyph_by_id(glyph, requested_size)
            }
        }
    }

    pub fn glyph_by_id(&self, glyph: GlyphId, requested_size: u16) -> Option<Glyph<'_>> {
        self.glyph_by_id_for_output(glyph, requested_size, requested_size)
    }

    pub(crate) fn glyph_by_id_for_output(
        &self,
        glyph: GlyphId,
        requested_size: u16,
        output_ppem: u16,
    ) -> Option<Glyph<'_>> {
        match &self.backend {
            FontBackend::Bitmap8x8 => {
                let value = u8::try_from(glyph.value()).ok()?;
                if !(b' '..0x7f).contains(&value) {
                    return None;
                }
                bitmap_8x8_glyph(value as char)
            }
            FontBackend::Custom(provider) => {
                let advance = provider.glyph_advance(glyph, requested_size)?;
                let raster = provider.raster(glyph, requested_size, output_ppem)?;
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

    pub(crate) fn glyph_run_ink_bounds(
        &self,
        glyphs: &[textflow::shaping::PositionedGlyph],
        pos: Point,
        transform: Transform,
        output_ppem: u16,
    ) -> Option<Rect> {
        let mut bounds = self.glyph_run_local_bounds(glyphs, output_ppem)?;
        bounds.x += pos.x;
        bounds.y += pos.y;
        Some(transform.apply_rect_bbox(bounds))
    }

    fn glyph_run_local_bounds(
        &self,
        glyphs: &[textflow::shaping::PositionedGlyph],
        output_ppem: u16,
    ) -> Option<Rect> {
        let first = glyphs.first()?;
        let requested_size = self.size.max(1);
        let metrics = self.metrics(requested_size);
        let mut bounds: Option<Rect> = None;
        for positioned in glyphs {
            let Some(glyph) =
                self.glyph_by_id_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let dx = positioned
                .origin
                .x
                .checked_sub(first.origin.x)?
                .checked_add(positioned.offset.x)?;
            let dy = positioned
                .origin
                .y
                .checked_sub(first.origin.y)?
                .checked_add(positioned.offset.y)?;
            let dx = crate::types::fixed::from_textflow(dx);
            let dy = crate::types::fixed::from_textflow(dy);
            let rect = match glyph.kind {
                GlyphKind::Mono(_) => Rect {
                    x: dx,
                    y: dy,
                    w: Fixed::from_int(bitmap_8x8::CHAR_W as i32),
                    h: metrics.line_height,
                },
                GlyphKind::Raster {
                    region,
                    representation,
                    bearing_x,
                    bearing_y,
                    ..
                } => {
                    if region.width() == 0 || region.height() == 0 {
                        continue;
                    }
                    let glyph_scale = Fixed::from_int(i32::from(requested_size))
                        / Fixed::from_int(i32::from(representation.design_ppem().max(1)));
                    Rect {
                        x: dx + bearing_x,
                        y: metrics.ascender + dy - bearing_y,
                        w: Fixed::from_int(i32::try_from(region.width()).ok()?) * glyph_scale,
                        h: Fixed::from_int(i32::try_from(region.height()).ok()?) * glyph_scale,
                    }
                }
            };
            bounds = Some(match bounds {
                Some(current) => current.union(&rect),
                None => rect,
            });
        }
        bounds
    }

    /// Cheap metrics — no glyph touch.
    pub fn metrics(&self, requested_size: u16) -> FontMetrics {
        match &self.backend {
            FontBackend::Bitmap8x8 => BITMAP_8X8_METRICS,
            FontBackend::Custom(p) => p.metrics(requested_size),
        }
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32")
    ))]
    pub(crate) fn line_origin_for_baseline(&self, baseline: Point) -> Point {
        Point {
            x: baseline.x,
            y: baseline.y - self.metrics(self.size).ascender,
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

    pub fn supports_complex_shaping(&self) -> bool {
        match &self.backend {
            FontBackend::Bitmap8x8 => false,
            FontBackend::Custom(provider) => provider.supports_complex_shaping(),
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
    language: Option<&'a str>,
    shaping: crate::ui::widgets::ShapingPolicy,
}

pub(crate) const MAX_RESOLVED_FONT_STACK: usize = if cfg!(feature = "std") { 64 } else { 8 };

pub(crate) struct ResolvedFontStack {
    fonts: [Option<Font>; MAX_RESOLVED_FONT_STACK],
    len: usize,
    ppem: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FontStackCapacityError {
    pub required: usize,
    pub limit: usize,
}

impl ResolvedFontStack {
    pub(crate) fn resolve(
        world: &World,
        stack: &FontStack,
        font_size: Option<u16>,
        limit: usize,
    ) -> Result<Option<Self>, FontStackCapacityError> {
        let len = stack.iter().count();
        let limit = limit.min(MAX_RESOLVED_FONT_STACK);
        if len > limit {
            return Err(FontStackCapacityError {
                required: len,
                limit,
            });
        }
        let Some(manager) = world.resource::<FontManager>() else {
            return Ok(None);
        };
        let primary = manager.resolve(stack.primary().cache_key());
        let ppem = font_size.unwrap_or(primary.size).max(1);
        let mut fonts = core::array::from_fn(|_| None);
        for (slot, token) in fonts.iter_mut().zip(stack.iter()) {
            let mut font = manager.resolve(token.cache_key()).as_ref().clone();
            font.size = ppem;
            *slot = Some(font);
        }
        Ok(Some(Self { fonts, len, ppem }))
    }

    pub(crate) fn primary(&self) -> &Font {
        self.fonts[0].as_ref().expect("font stack has a primary")
    }

    pub(crate) fn font(&self, id: FontFaceId) -> Option<&Font> {
        self.fonts[..self.len]
            .iter()
            .filter_map(Option::as_ref)
            .find(|font| font.face_id() == id)
    }

    pub(crate) fn layout_fingerprint(
        &self,
        language: Option<&str>,
        shaping: crate::ui::widgets::ShapingPolicy,
    ) -> u64 {
        let mut fingerprint = 0xcbf2_9ce4_8422_2325_u64;
        let mut write = |bytes: &[u8]| {
            for byte in bytes {
                fingerprint ^= u64::from(*byte);
                fingerprint = fingerprint.wrapping_mul(0x100_0000_01b3);
            }
        };
        write(&self.ppem.to_le_bytes());
        write(&[match shaping {
            crate::ui::widgets::ShapingPolicy::Auto => 0,
            crate::ui::widgets::ShapingPolicy::Simple => 1,
            crate::ui::widgets::ShapingPolicy::Required => 2,
        }]);
        if let Some(language) = language {
            write(language.as_bytes());
        }
        for font in self.fonts[..self.len].iter().filter_map(Option::as_ref) {
            write(&font.face_id().value().to_le_bytes());
            write(&font.revision().to_le_bytes());
            let storage = match &font.backend {
                FontBackend::Bitmap8x8 => 0,
                FontBackend::Custom(provider) => Rc::as_ptr(provider) as *const () as usize as u64,
            };
            write(&storage.to_le_bytes());
        }
        fingerprint
    }

    pub(crate) fn with_typefaces<'a, R>(
        &'a self,
        language: Option<&'a str>,
        shaping: crate::ui::widgets::ShapingPolicy,
        f: impl FnOnce(&[&dyn textflow::shaping::Typeface]) -> R,
    ) -> R {
        let primary = self.primary();
        let faces: [FontTypeface<'_>; MAX_RESOLVED_FONT_STACK] = core::array::from_fn(|index| {
            let font = self.fonts[index].as_ref().unwrap_or(primary);
            FontTypeface::new(font, self.ppem)
                .with_language(language)
                .with_shaping(shaping)
        });
        let typefaces: [&dyn textflow::shaping::Typeface; MAX_RESOLVED_FONT_STACK] =
            core::array::from_fn(|index| &faces[index] as &dyn textflow::shaping::Typeface);
        f(&typefaces[..self.len])
    }
}

impl<'a> FontTypeface<'a> {
    pub const fn new(font: &'a Font, ppem: u16) -> Self {
        Self {
            font,
            ppem,
            language: None,
            shaping: crate::ui::widgets::ShapingPolicy::Auto,
        }
    }

    pub const fn with_language(mut self, language: Option<&'a str>) -> Self {
        self.language = language;
        self
    }

    pub const fn with_shaping(mut self, shaping: crate::ui::widgets::ShapingPolicy) -> Self {
        self.shaping = shaping;
        self
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
        if self.shaping == crate::ui::widgets::ShapingPolicy::Required
            && !self.font.supports_complex_shaping()
        {
            return Ok(false);
        }
        Ok(self.font.covers(cluster))
    }

    fn supports_complex_shaping(&self) -> bool {
        self.shaping != crate::ui::widgets::ShapingPolicy::Simple
            && self.font.supports_complex_shaping()
    }

    fn shape_into(
        &self,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        let mut adjusted = textflow::shaping::ShapeRequest::new(
            request.text,
            request.range.clone(),
            request.direction,
            request.script,
        )
        .with_features(request.features)
        .with_line_edges(request.line_edges);
        if let Some(language) = self.language.or(request.language) {
            adjusted = adjusted.with_language(language);
        }
        match self.shaping {
            crate::ui::widgets::ShapingPolicy::Auto => {
                self.font.shape_into(self.ppem, &adjusted, output)
            }
            crate::ui::widgets::ShapingPolicy::Required => {
                if !self.font.supports_complex_shaping() {
                    return Err(textflow::shaping::ShapeError::ShapingUnavailable);
                }
                self.font.shape_into(self.ppem, &adjusted, output)
            }
            crate::ui::widgets::ShapingPolicy::Simple => textflow::shaping::Typeface::shape_into(
                &textflow::shaping::SimpleTypeface::new(&FontGlyphSource {
                    font: self.font,
                    ppem: self.ppem,
                }),
                &adjusted,
                output,
            ),
        }
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
pub(crate) fn default_font_manager() -> FontManager {
    ResourceManager::new(crate::core::cache::MaxSize::Unbound, Font::bitmap_8x8())
}

/// Resolve `token` against the World's [`FontManager`], or `None` when
/// the manager has not been inserted yet. Returns an owned `Rc<Font>`
/// so the `&World` borrow ends at the call — the render path holds the
/// `Rc` locally instead of borrowing through the manager's `RefCell`.
pub(crate) fn resolve_or_default(world: &World, token: &FontToken) -> Option<Rc<Font>> {
    world
        .resource::<FontManager>()
        .map(|m| m.resolve(token.cache_key()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32")
    ))]
    #[test]
    fn cached_run_origin_maps_a_baseline_to_a_line_top() {
        let font = Font::bitmap_8x8();
        let baseline = Point::new(11, 21);
        let origin = font.line_origin_for_baseline(baseline);
        assert_eq!(origin.x, baseline.x);
        assert_eq!(origin.y + font.metrics(font.size).ascender, baseline.y);
    }

    #[cfg(any(feature = "sdl-gpu", feature = "wgpu"))]
    #[test]
    fn glyph_raster_extent_preserves_fractional_display_scale() {
        assert_eq!(
            scaled_glyph_raster_extent(17, Fixed::from_ratio(3, 2)),
            Some(26)
        );
        assert_eq!(
            scaled_glyph_raster_extent(17, Fixed::from_ratio(5, 4)),
            Some(22)
        );
    }

    #[cfg(feature = "sdl-gpu")]
    #[test]
    fn raster_run_bounds_quantize_after_fractional_scaling() {
        let scale = Fixed::from_ratio(3, 2);
        let bounds = RasterRunBounds::from_logical(
            crate::types::Rect {
                x: Fixed::from_ratio(-1, 2),
                y: Fixed::from_ratio(1, 4),
                w: Fixed::from_ratio(21, 2),
                h: Fixed::from_ratio(15, 2),
            },
            scale,
        )
        .unwrap();

        assert_eq!((bounds.width, bounds.height), (16, 12));
        assert_eq!(bounds.offset.x, Fixed::from_int(-1) / scale);
        assert_eq!(bounds.offset.y, Fixed::ZERO);
        assert_eq!(bounds.size.x, Fixed::from_int(16) / scale);
        assert_eq!(bounds.size.y, Fixed::from_int(12) / scale);
    }

    #[cfg(feature = "sdl-gpu")]
    #[test]
    fn raster_run_key_uses_font_identity_but_not_composite_alpha() {
        let font = Font::bitmap_8x8();
        let scale = Fixed::from_ratio(3, 2);
        let faint = font.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 40), scale);
        let opaque = font.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 255), scale);
        assert_eq!(faint, opaque);

        let mut larger = font.clone();
        larger.size += 1;
        assert_ne!(
            faint,
            larger.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 40), scale)
        );
    }

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

    #[cfg(feature = "wgpu")]
    #[test]
    fn bitmap_font_exposes_the_shared_coverage_surface_to_gpu_backends() {
        let font = Font::bitmap_8x8();
        let raster = font
            .raster_for_output(GlyphId::new(u16::from(b'A')), 8, 16)
            .unwrap();

        assert_eq!(
            raster.surface.sample_layout(),
            ::mirx::image::SampleLayout::A1
        );
        assert_eq!(raster.surface.width(), 8);
        assert_eq!(raster.surface.height(), 8 * 95);
        assert_eq!(raster.region.unwrap().y(), u32::from(b'A' - b' ') * 8);
        assert_eq!(raster.representation.design_ppem(), 8);
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
            fn raster(
                &self,
                _glyph: GlyphId,
                _layout_ppem: u16,
                _output_ppem: u16,
            ) -> Option<RasterGlyph<'_>> {
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

    #[test]
    fn paragraph_language_reaches_the_font_adapter() {
        struct LanguageProbe;

        impl FontProvider for LanguageProbe {
            fn face_id(&self) -> FontFaceId {
                FontFaceId::new(3)
            }

            fn map_char(&self, _ch: char) -> Option<GlyphId> {
                Some(GlyphId::new(1))
            }

            fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
                Some(Fixed::from_int(6))
            }

            fn raster(
                &self,
                _glyph: GlyphId,
                _layout_ppem: u16,
                _output_ppem: u16,
            ) -> Option<RasterGlyph<'_>> {
                None
            }

            fn metrics(&self, _ppem: u16) -> FontMetrics {
                BITMAP_8X8_METRICS
            }

            fn shape_into(
                &self,
                _ppem: u16,
                request: &textflow::shaping::ShapeRequest<'_>,
                output: &mut [textflow::shaping::ShapedGlyph],
            ) -> Result<usize, textflow::shaping::ShapeError> {
                assert_eq!(request.language, Some("zh-Hans"));
                output[0] = textflow::shaping::ShapedGlyph::new(
                    GlyphId::new(1),
                    textflow::shaping::TextRange::new(0, 1),
                );
                Ok(1)
            }
        }

        let font = Font {
            family: "language-probe",
            size: 8,
            backend: FontBackend::Custom(Rc::new(LanguageProbe)),
        };
        let face = FontTypeface::new(&font, 8).with_language(Some("zh-Hans"));
        let request = textflow::shaping::ShapeRequest::new(
            "A",
            0..1,
            textflow::bidi::Direction::LeftToRight,
            textflow::unicode::Script::Latin,
        );
        let mut output = [textflow::shaping::ShapedGlyph::default(); 1];

        assert_eq!(
            textflow::shaping::Typeface::shape_into(&face, &request, &mut output),
            Ok(1)
        );
    }

    #[test]
    fn required_shaping_rejects_simple_faces() {
        let font = Font::bitmap_8x8();
        let face =
            FontTypeface::new(&font, 8).with_shaping(crate::ui::widgets::ShapingPolicy::Required);
        let request = textflow::shaping::ShapeRequest::new(
            "A",
            0..1,
            textflow::bidi::Direction::LeftToRight,
            textflow::unicode::Script::Latin,
        );
        let mut output = [textflow::shaping::ShapedGlyph::default(); 1];

        assert_eq!(
            textflow::shaping::Typeface::shape_into(&face, &request, &mut output),
            Err(textflow::shaping::ShapeError::ShapingUnavailable)
        );
    }

    #[test]
    fn simple_policy_bypasses_complex_shaping() {
        struct ComplexProbe;

        impl FontProvider for ComplexProbe {
            fn face_id(&self) -> FontFaceId {
                FontFaceId::new(4)
            }

            fn map_char(&self, _ch: char) -> Option<GlyphId> {
                Some(GlyphId::new(1))
            }

            fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
                Some(Fixed::from_int(6))
            }

            fn raster(
                &self,
                _glyph: GlyphId,
                _layout_ppem: u16,
                _output_ppem: u16,
            ) -> Option<RasterGlyph<'_>> {
                None
            }

            fn metrics(&self, _ppem: u16) -> FontMetrics {
                BITMAP_8X8_METRICS
            }

            fn supports_complex_shaping(&self) -> bool {
                true
            }

            fn shape_into(
                &self,
                _ppem: u16,
                request: &textflow::shaping::ShapeRequest<'_>,
                output: &mut [textflow::shaping::ShapedGlyph],
            ) -> Result<usize, textflow::shaping::ShapeError> {
                output[0] = textflow::shaping::ShapedGlyph::new(
                    GlyphId::new(77),
                    textflow::shaping::TextRange::new(
                        request.range.start as u32,
                        request.range.end as u32,
                    ),
                );
                Ok(1)
            }
        }

        let font = Font {
            family: "complex-probe",
            size: 8,
            backend: FontBackend::Custom(Rc::new(ComplexProbe)),
        };
        let request = textflow::shaping::ShapeRequest::new(
            "A",
            0..1,
            textflow::bidi::Direction::LeftToRight,
            textflow::unicode::Script::Latin,
        );
        let mut output = [textflow::shaping::ShapedGlyph::default(); 1];
        let automatic = FontTypeface::new(&font, 8);
        assert_eq!(
            textflow::shaping::Typeface::shape_into(&automatic, &request, &mut output),
            Ok(1)
        );
        assert_eq!(output[0].glyph_id(), GlyphId::new(77));

        let simple =
            FontTypeface::new(&font, 8).with_shaping(crate::ui::widgets::ShapingPolicy::Simple);
        assert_eq!(
            textflow::shaping::Typeface::shape_into(&simple, &request, &mut output),
            Ok(1)
        );
        assert_eq!(output[0].glyph_id(), GlyphId::new(1));
    }
}
