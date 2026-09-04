//! One-face MIRX font provider with representation-specific metrics and geometry.

use alloc::rc::Rc;

use super::{Font, FontBackend, FontMetrics, FontProvider, Glyph, GlyphKind};
use mirx::font::FontGlyphs;
use mirx::{
    FontError, FontRepresentationFallback, FontRepresentationRequest, FontView, PayloadLimits,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MirxFontError {
    Container,
    MissingFace,
    MultipleFaces,
    InvalidFace(FontError),
    EncodedStorage,
}

#[derive(Clone, Copy, Debug)]
pub struct MirxFontProvider {
    face: FontView<'static>,
    default_size: u16,
}

impl MirxFontProvider {
    /// Opens exactly one FONT face from a complete MIRX file.
    pub fn from_mirx(bytes: &'static [u8], limits: &PayloadLimits) -> Result<Self, MirxFontError> {
        let reader = mirx::Reader::open(bytes).map_err(|_| MirxFontError::Container)?;
        let mut faces = reader
            .chunks()
            .filter(|chunk| chunk.chunk_type() == mirx::ChunkType::FONT);
        let chunk = faces.next().ok_or(MirxFontError::MissingFace)?;
        if faces.next().is_some() {
            return Err(MirxFontError::MultipleFaces);
        }
        let face = chunk
            .font(limits)
            .map_err(MirxFontError::InvalidFace)?
            .expect("FONT filter");
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_view(face)
    }

    /// Opens one standalone FONT payload whose backing storage is static.
    pub fn from_payload(
        payload: &'static [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, MirxFontError> {
        let face = FontView::open(payload, limits).map_err(MirxFontError::InvalidFace)?;
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_view(face)
    }

    fn from_view(face: FontView<'static>) -> Result<Self, MirxFontError> {
        for index in 0..face.tables().len() {
            if matches!(face.glyphs(index), Some(FontGlyphs::Encoded(_))) {
                return Err(MirxFontError::EncodedStorage);
            }
        }
        let representations = face.tables().representations();
        let default_size = representations
            .iter()
            .map(|record| record.representation())
            .filter(|representation| {
                matches!(
                    representation.kind(),
                    mirx::FontRepresentationKind::Coverage { .. }
                )
            })
            .map(|representation| representation.design_ppem())
            .max()
            .unwrap_or_else(|| {
                representations
                    .get(0)
                    .expect("nonempty admitted face")
                    .representation()
                    .design_ppem()
            });
        Ok(Self { face, default_size })
    }

    pub const fn default_size(self) -> u16 {
        self.default_size
    }
    pub const fn view(self) -> FontView<'static> {
        self.face
    }

    fn selected(&self, size: u16) -> Option<mirx::font::FaceRepresentation<'static>> {
        self.face
            .tables()
            .select(
                FontRepresentationRequest::new(size)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .ok()
    }
}

impl FontProvider for MirxFontProvider {
    fn glyph(&self, ch: char, requested_size: u16) -> Option<Glyph> {
        let selected = self.selected(requested_size)?;
        let ordinal = self.face.tables().codepoints().binary_search(ch).ok()?;
        let metric = selected.metrics().get(ordinal)?;
        let FontGlyphs::Raw(storage) = self.face.glyphs(selected.index())? else {
            return None;
        };
        let raster = storage.get(ordinal)?;
        let plane = raster.storage().plane(0)?;
        Some(Glyph {
            advance: crate::types::Fixed::from_raw(metric.advance().raw()),
            kind: GlyphKind::Raster {
                samples: plane.bytes(),
                stride: plane.memory().stride(),
                region: raster.region(),
                representation: selected.record().representation(),
                bearing_x: crate::types::Fixed::from_raw(metric.bearing_x().raw()),
                bearing_y: crate::types::Fixed::from_raw(metric.bearing_y().raw()),
            },
        })
    }

    fn metrics(&self, requested_size: u16) -> FontMetrics {
        self.selected(requested_size)
            .map(|selected| {
                let metrics = selected.metrics().line_metrics();
                let scale = crate::types::Fixed::from_int(i32::from(requested_size))
                    / crate::types::Fixed::from_int(i32::from(
                        selected.record().representation().design_ppem(),
                    ));
                FontMetrics {
                    ascender: crate::types::Fixed::from_raw(metrics.ascent().raw()) * scale,
                    descender: crate::types::Fixed::from_raw(metrics.descent().raw()) * scale,
                    line_height: crate::types::Fixed::from_raw(metrics.line_height().raw()) * scale,
                }
            })
            .unwrap_or(FontMetrics {
                ascender: crate::types::Fixed::ZERO,
                descender: crate::types::Fixed::ZERO,
                line_height: crate::types::Fixed::ONE,
            })
    }
}

pub fn font_from_mirx(
    family: &'static str,
    bytes: &'static [u8],
    limits: &PayloadLimits,
) -> Result<Font, MirxFontError> {
    let provider = MirxFontProvider::from_mirx(bytes, limits)?;
    Ok(Font {
        family,
        size: provider.default_size(),
        backend: FontBackend::Custom(Rc::new(provider)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirx::{
        Fixed,
        coding::Rle,
        font::{
            FontAsset, GlyphMap, GlyphMetrics, GlyphSurfaceAsset, LineMetrics, RawGlyphs,
            RepresentationAsset,
        },
        image::{
            ColorDescription, EncodedImageAsset, PlaneMemoryLayout, Region, SampleLayout,
            SurfaceDescriptor,
        },
    };

    fn atlas_face() -> MirxFontProvider {
        let codepoints = [' ', 'A'];
        let regions = [
            Region::new(0, 0, 0, 0).unwrap(),
            Region::new(3, 1, 3, 2).unwrap(),
        ];
        let map = GlyphMap::atlas(8, 4, &regions).unwrap();
        let surface =
            SurfaceDescriptor::new(8, 4, SampleLayout::A1, ColorDescription::NONE).unwrap();
        let memory = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_stride(64)
            .with_alignment(64)
            .build()
            .unwrap();
        let samples = [0xa5; 256];
        let glyphs = RawGlyphs::builder(map, SampleLayout::A1)
            .with_memory_layout(memory)
            .build(&samples)
            .unwrap();
        let metrics = [
            GlyphMetrics::new(Fixed::from_int(4), Fixed::ZERO, Fixed::from_int(12)),
            GlyphMetrics::new(
                Fixed::from_raw(5 * 256 + 128),
                Fixed::from_raw(-128),
                Fixed::from_raw(11 * 256 + 64),
            ),
        ];
        let line = LineMetrics::new(
            Fixed::from_int(12),
            Fixed::from_int(-4),
            Fixed::from_int(16),
        )
        .unwrap();
        let representation = RepresentationAsset::new(
            mirx::FontRepresentation::coverage(1, 16, 4).unwrap(),
            0,
            line,
            &metrics,
        )
        .with_map(0);
        let payload = FontAsset::new(
            &codepoints,
            &[representation],
            &[GlyphSurfaceAsset::raw(glyphs)],
        )
        .with_maps(&[map])
        .encode()
        .unwrap();
        MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST)
            .unwrap()
    }

    #[test]
    fn atlas_regions_and_fractional_metrics_reach_the_renderer_unchanged() {
        let provider = atlas_face();
        let glyph = provider.glyph('A', 16).unwrap();
        assert_eq!(glyph.advance.raw(), 5 * 256 + 128);
        let GlyphKind::Raster {
            stride,
            region,
            bearing_x,
            bearing_y,
            ..
        } = glyph.kind
        else {
            panic!("raster glyph");
        };
        assert_eq!(stride, 64);
        assert_eq!(
            (region.x(), region.y(), region.width(), region.height()),
            (3, 1, 3, 2)
        );
        assert_eq!(bearing_x.raw(), -128);
        assert_eq!(bearing_y.raw(), 11 * 256 + 64);
        assert!(matches!(
            provider.glyph(' ', 16).unwrap().kind,
            GlyphKind::Raster { region, .. } if region.is_empty()
        ));
        assert!(provider.glyph('Z', 16).is_none());
    }

    #[test]
    fn size_specific_line_and_glyph_metrics_use_one_representation() {
        let provider = atlas_face();
        let metrics = provider.metrics(8);
        assert_eq!(metrics.ascender, crate::types::Fixed::from_int(6));
        assert_eq!(metrics.descender, crate::types::Fixed::from_int(-2));
        assert_eq!(metrics.line_height, crate::types::Fixed::from_int(8));
        assert_eq!(provider.glyph('A', 8).unwrap().advance.raw(), 5 * 256 + 128);
    }

    #[test]
    fn encoded_surfaces_require_an_explicit_decode_workspace_provider() {
        let codepoints = ['A', 'B'];
        let metrics = [GlyphMetrics::default(); 2];
        let line = LineMetrics::new(Fixed::ZERO, Fixed::ZERO, Fixed::ONE).unwrap();
        let map = GlyphMap::glyph_major(2, 2, codepoints.len()).unwrap();
        let surface =
            SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let image = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
        let representation = RepresentationAsset::new(
            mirx::FontRepresentation::coverage(8, 1, 8).unwrap(),
            0,
            line,
            &metrics,
        );
        let payload = FontAsset::new(
            &codepoints,
            &[representation],
            &[GlyphSurfaceAsset::Encoded { map, image }],
        )
        .encode()
        .unwrap();
        assert!(matches!(
            MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST),
            Err(MirxFontError::EncodedStorage)
        ));
    }
}
