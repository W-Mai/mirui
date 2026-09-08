use super::{GlyphSurfaceRecord, GlyphSurfaceRecordError};
use crate::{
    font::{GlyphMap, GlyphPacking, RawGlyphs},
    image::{PLANE_RECORD_LEN, PlaneMemoryLayout},
    media::MediaPayload,
};

impl GlyphSurfaceRecord {
    /// Binds referenced RAW samples to a matching map without scanning DATA.
    ///
    /// Directory identity, map geometry, physical allocation and the exact DATA
    /// span are checked. Complete face admission or a selected read plan must
    /// establish DATA integrity separately. File and actual-address alignment
    /// remain explicit checks on the returned storage.
    ///
    /// ```
    /// use mirx::{font::{GlyphMap, GlyphPacking, GlyphSurfaceRecord},
    ///     image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor},
    ///     media::MediaPayload};
    /// let surface = SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let bytes = RawImageAsset::new(surface, &[&[1, 2, 3, 4]]).encode().unwrap();
    /// let media = MediaPayload::open(&bytes).unwrap();
    /// media.validate_data().unwrap();
    /// let record = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 1, 1).unwrap();
    /// let glyphs = record.raw_glyphs(media, GlyphMap::cells(2, 1, 2).unwrap()).unwrap();
    /// assert_eq!(glyphs.get(1).unwrap().storage().plane(0).unwrap().bytes(), &[3, 4]);
    /// ```
    pub fn raw_glyphs<'map, 'data>(
        self,
        media: MediaPayload<'data>,
        map: GlyphMap<'map>,
    ) -> Result<RawGlyphs<'map, 'data>, GlyphSurfaceRecordError> {
        if self.codings_section().is_some() {
            return Err(GlyphSurfaceRecordError::ExpectedRawStorage);
        }
        self.validate_sections(media)?;
        self.validate_map(map)?;
        if !self.sample_layout().is_alpha() {
            return Err(GlyphSurfaceRecordError::UnsupportedLayout(
                self.sample_layout(),
            ));
        }
        let mut builder = RawGlyphs::builder(map, self.sample_layout());
        if let Some(index) = self.planes_section() {
            let bytes = media
                .get(usize::from(index))
                .expect("validated plane reference")
                .bytes();
            if bytes.len() != PLANE_RECORD_LEN {
                return Err(GlyphSurfaceRecordError::PlaneRecordLength {
                    actual: bytes.len(),
                });
            }
            let geometry = self
                .sample_layout()
                .plane_geometry(self.width(), self.height(), 0)
                .expect("scalar cell or atlas geometry");
            let memory = PlaneMemoryLayout::from_record(geometry, bytes)
                .map_err(GlyphSurfaceRecordError::Plane)?;
            builder = builder.with_memory_layout(memory);
        }
        let data = media
            .get(usize::from(self.data_section()))
            .expect("validated DATA reference");
        builder
            .build(data.bytes())
            .map_err(GlyphSurfaceRecordError::Storage)
    }

    pub(super) fn validate_map(self, map: GlyphMap<'_>) -> Result<(), GlyphSurfaceRecordError> {
        let extent = match map.packing() {
            GlyphPacking::GlyphMajor => map.cell_extent().expect("fixed cell map"),
            GlyphPacking::Atlas2D => (map.width(), map.height()),
        };
        if self.packing() != map.packing() || (self.width(), self.height()) != extent {
            return Err(GlyphSurfaceRecordError::RasterMapMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
