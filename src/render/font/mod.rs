//! Font resources, glyph identity and raster access.
//!
//! [`FontManager`] (a `ResourceManager<Font>`) is the World resource
//! that maps a [`FontToken`] cache key to a concrete [`Font`]. Built-in
//! widgets resolve a [`FontStack`] through the manager. The fallback provider
//! is an embedded MIRX face; external font formats plug in through
//! [`FontProvider`].

pub mod mirx;
mod pixel_grid;
pub(crate) mod scalar;
pub mod sdf;

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
    fn local_rect(self, requested_size: u16) -> Option<Rect> {
        let region = self
            .region
            .filter(|region| region.width() > 0 && region.height() > 0)?;
        let scale = Fixed::from_int(i32::from(requested_size))
            / Fixed::from_int(i32::from(self.representation.design_ppem().max(1)));
        Some(Rect {
            x: self.offset_x,
            y: Fixed::ZERO - self.offset_y,
            w: Fixed::from_int(i32::try_from(region.width()).ok()?) * scale,
            h: Fixed::from_int(i32::try_from(region.height()).ok()?) * scale,
        })
    }

    #[cfg(any(feature = "wgpu", test))]
    pub(crate) fn positioned_quad(
        self,
        pos: Point,
        first_origin: textflow::shaping::FlowPoint,
        glyph: &textflow::shaping::PositionedGlyph,
        requested_size: u16,
        ascender: Fixed,
        transform: Transform,
    ) -> Option<RasterQuad> {
        let dx = glyph
            .origin
            .x
            .checked_sub(first_origin.x)?
            .checked_add(glyph.offset.x)?;
        let dy = glyph
            .origin
            .y
            .checked_sub(first_origin.y)?
            .checked_add(glyph.offset.y)?;
        let mut rect = self.local_rect(requested_size)?;
        rect.x = pos.x + crate::types::fixed::from_textflow(dx) + self.offset_x;
        rect.y = pos.y + ascender + crate::types::fixed::from_textflow(dy) - self.offset_y;
        Some(RasterQuad { rect, transform })
    }

    pub(crate) fn posed_quad(
        self,
        pos: Point,
        frame: textflow::placement::GlyphFrame,
        requested_size: u16,
        transform: Transform,
    ) -> Option<RasterQuad> {
        let rect = self.local_rect(requested_size)?;
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
        Some(RasterQuad {
            rect,
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

pub(crate) fn raster_glyph_origin(
    physical_pos: Point,
    first: textflow::shaping::FlowPoint,
    glyph: &textflow::shaping::PositionedGlyph,
    ascender: Fixed,
    scale: Fixed,
) -> Option<(i32, i32)> {
    let dx = glyph
        .origin
        .x
        .checked_sub(first.x)?
        .checked_add(glyph.offset.x)?;
    let dy = glyph
        .origin
        .y
        .checked_sub(first.y)?
        .checked_add(glyph.offset.y)?;
    Some((
        physical_pos.x.floor().to_int() + (crate::types::fixed::from_textflow(dx) * scale).to_int(),
        physical_pos.y.floor().to_int()
            + (ascender * scale).to_int()
            + (crate::types::fixed::from_textflow(dy) * scale).to_int(),
    ))
}

pub(crate) fn raster_glyph_rect(
    origin: (i32, i32),
    bearing: Point,
    region: ::mirx::image::Region,
    requested_size: u16,
    design_ppem: u16,
    scale: Fixed,
) -> (i32, i32, u16, u16) {
    let glyph_scale = Fixed::from_int(i32::from(requested_size))
        / Fixed::from_int(i32::from(design_ppem.max(1)))
        * scale;
    let extent = |value: u32| {
        let raw_scale =
            u64::try_from(crate::types::fixed::storage::to_i32(glyph_scale)).unwrap_or(0);
        (u64::from(value) * raw_scale)
            .div_ceil(256)
            .clamp(1, u64::from(u16::MAX)) as u16
    };
    (
        origin.0 + (bearing.x * scale).to_int(),
        origin.1 - (bearing.y * scale).to_int(),
        extent(region.width()),
        extent(region.height()),
    )
}

/// Format adapter used by both paragraph layout and render backends.
pub trait FontProvider: 'static {
    fn face_id(&self) -> FontFaceId;
    /// Revision of coverage, metrics, shaping and glyph data. Mutable providers advance this value when those inputs change; immutable providers can keep the default.
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

    /// The size this face lays out and draws a text that asks for `ppem`.
    /// Faces that draw at any size keep it.
    fn snap_ppem(&self, ppem: u16) -> u16 {
        ppem
    }

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

/// A font resource with shared provider storage and logical pixel size.
#[derive(Clone)]
pub struct Font {
    pub family: &'static str,
    pub size: u16,
    pub provider: Rc<dyn FontProvider>,
}

impl core::fmt::Debug for Font {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Font")
            .field("family", &self.family)
            .field("size", &self.size)
            .field("face_id", &self.face_id())
            .finish_non_exhaustive()
    }
}

impl Default for Font {
    /// Opens the embedded ASCII MIRX font at 8 px; clones share its provider.
    fn default() -> Self {
        Self::from_mirx(
            "default",
            8,
            include_bytes!("assets/default.mirx"),
            &::mirx::reader::PayloadLimits::EMBEDDED,
        )
        .expect("embedded font is validated during asset generation")
    }
}

impl Font {
    pub fn face_id(&self) -> FontFaceId {
        self.provider.face_id()
    }

    pub fn revision(&self) -> u64 {
        self.provider.revision()
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
        self.provider.map_char(ch)
    }

    pub fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<Fixed> {
        self.provider.glyph_advance(glyph, ppem)
    }

    pub fn raster(&self, glyph: GlyphId, ppem: u16) -> Option<RasterGlyph<'_>> {
        self.provider.raster(glyph, ppem, ppem)
    }

    pub(crate) fn raster_for_output(
        &self,
        glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        self.provider.raster(glyph, layout_ppem, output_ppem)
    }

    pub fn glyph(&self, ch: char, requested_size: u16) -> Option<Glyph<'_>> {
        let glyph = self
            .provider
            .map_char(ch)
            .or_else(|| self.provider.notdef_glyph())?;
        self.glyph_by_id(glyph, requested_size)
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
        let advance = self.provider.glyph_advance(glyph, requested_size)?;
        let raster = self.provider.raster(glyph, requested_size, output_ppem)?;
        let region = raster
            .region
            .unwrap_or_else(|| ::mirx::image::Region::new(0, 0, 0, 0).expect("empty glyph region"));
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
        self.provider.metrics(requested_size)
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
        self.provider.covers(cluster)
    }

    pub fn shape_into(
        &self,
        ppem: u16,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        self.provider.shape_into(ppem, request, output)
    }

    pub fn supports_complex_shaping(&self) -> bool {
        self.provider.supports_complex_shaping()
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
        manager: Option<&FontManager>,
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
        let Some(manager) = manager else {
            return Ok(None);
        };
        // A registered FALLBACK face closes the stack unless it already
        // names it or has no room left.
        let backing = (len < limit && stack.iter().all(|token| *token != FontToken::FALLBACK))
            .then(|| manager.resolve_registered(FontToken::FALLBACK.cache_key()))
            .flatten();
        let primary = manager.resolve(stack.primary().cache_key());
        let ppem = primary.ppem_for(font_size);
        let mut fonts = core::array::from_fn(|_| None);
        for (slot, token) in fonts.iter_mut().zip(stack.iter()) {
            let mut font = manager.resolve(token.cache_key()).as_ref().clone();
            font.size = ppem;
            *slot = Some(font);
        }
        let len = if let Some(backing) = backing {
            let mut font = backing.as_ref().clone();
            font.size = ppem;
            fonts[len] = Some(font);
            len + 1
        } else {
            len
        };
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
            let storage = Rc::as_ptr(&font.provider) as *const () as usize as u64;
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
    /// The face that backs every text stack: when a [`FontManager`] has a
    /// font registered under this token, each resolved stack draws glyphs
    /// its own faces lack from it. A Latin UI face can lean on a CJK face
    /// this way without every style listing the fallback.
    pub const FALLBACK: FontToken = FontToken::Custom("\0mirui-font-fallback");

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
/// The manager's fallback is [`Font::default`], so any unregistered
/// token resolves to the embedded MIRX font — widgets never see a missing
/// font.
pub type FontManager = ResourceManager<Font>;

/// Build the default font manager: an unbounded-budget
/// [`ResourceManager`] whose fallback is the embedded 8 px MIRX font.
pub(crate) fn default_font_manager() -> FontManager {
    ResourceManager::new(crate::core::cache::MaxSize::Unbound, Font::default())
}

/// Resolve `token` against the World's [`FontManager`], returning `None` when no manager is installed. The value handle retains the font without borrowing the manager during rendering.
pub(crate) fn resolve_or_default(
    world: &World,
    token: &FontToken,
) -> Option<crate::core::cache::Handle<Font>> {
    world
        .resource::<FontManager>()
        .map(|m| m.resolve(token.cache_key()))
}

#[cfg(test)]
#[path = "../../../tests/unit/render/font/mod.rs"]
mod tests;
