use super::*;
use crate::{
    Fixed, PayloadLimits,
    font::{FontRepresentation, FontRepresentationFallback, GlyphMetrics, LineMetrics},
    image::{Region, SampleLayout},
};

struct Fixture {
    codepoints: [u8; 8],
    records: [u8; 64],
    surfaces: [u8; 48],
    metrics: [u8; 144],
    maps: [u8; 32],
}

impl Fixture {
    fn new() -> Self {
        let mut result = Self {
            codepoints: [0; 8],
            records: [0; 64],
            surfaces: [0; 48],
            metrics: [0; 144],
            maps: [0; 32],
        };
        result.codepoints[..4].copy_from_slice(&('A' as u32).to_le_bytes());
        result.codepoints[4..].copy_from_slice(&('🦀' as u32).to_le_bytes());
        for (index, (packing, layout)) in [
            (GlyphPacking::GlyphMajor, SampleLayout::A4),
            (GlyphPacking::Atlas2D, SampleLayout::A8),
        ]
        .into_iter()
        .enumerate()
        {
            GlyphSurfaceRecord::new(layout, packing, 8, 8, 0)
                .unwrap()
                .encode_record_into(&mut result.surfaces[index * 24..])
                .unwrap();
        }
        for (index, metadata) in [
            FontRepresentation::coverage(4, 12, 64).unwrap(),
            FontRepresentation::coverage(4, 16, 64).unwrap(),
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 64).unwrap(),
            FontRepresentation::signed_distance(8, 5, 64, 49, 128, 64).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            RepresentationRecord::new(metadata, u16::from(index > 1))
                .encode_record_into(&mut result.records[index * 16..])
                .unwrap();
            let ppem = i32::from(metadata.design_ppem());
            let metrics = &mut result.metrics[index * 36..(index + 1) * 36];
            LineMetrics::new(
                Fixed::from_ratio(ppem * 192, 256),
                Fixed::from_ratio(-ppem * 64, 256),
                Fixed::from_int(ppem),
            )
            .unwrap()
            .encode_record_into(metrics)
            .unwrap();
            for glyph in 0..2 {
                GlyphMetrics::new(
                    Fixed::from_ratio(ppem * 128 + glyph as i32, 256),
                    Fixed::from_ratio(-257, 256),
                    Fixed::from_ratio(ppem * 256 + 511, 256),
                )
                .encode_record_into(&mut metrics[12 + glyph * 12..])
                .unwrap();
            }
        }
        GlyphMap::atlas(
            8,
            8,
            &[
                Region::new(1, 2, 3, 4).unwrap(),
                Region::new(8, 8, 0, 0).unwrap(),
            ],
        )
        .unwrap()
        .encode_into(&mut result.maps)
        .unwrap();
        result
    }

    fn table(&self) -> RepresentationTable<'_> {
        RepresentationTable::open(&self.records, &self.surfaces, 2, &PayloadLimits::EMBEDDED)
            .unwrap()
    }
    fn bind(&self) -> Result<FaceTables<'_>, FaceTablesError> {
        FaceTables::new(
            FontCodepoints::open(&self.codepoints).unwrap(),
            self.table(),
            &self.metrics,
            &self.maps,
        )
    }
}

#[test]
fn size_selection_keeps_shared_ordinals_metrics_and_regions_together() {
    let fixture = Fixture::new();
    let face = fixture.bind().unwrap();
    assert_eq!(face.len(), 4);
    assert!(!face.is_empty());
    assert_eq!(face.glyph_count(), 2);
    assert_eq!(face.codepoints().binary_search('🦀'), Ok(1));
    for (ppem, index) in [(12, 0), (16, 1), (17, 2), (48, 2), (49, 3), (128, 3)] {
        let chosen = face.select(FontRepresentationRequest::new(ppem)).unwrap();
        assert_eq!(chosen.index(), index);
        assert!(!chosen.used_fallback());
        assert_eq!(chosen.record(), face.representations().get(index).unwrap());
        let design = i32::from(chosen.record().representation().design_ppem());
        assert_eq!(
            chosen.metrics().line_metrics().line_height(),
            Fixed::from_int(design)
        );
        assert_eq!(
            chosen.metrics().get(1).unwrap().advance(),
            Fixed::from_ratio(design * 128 + 1, 256)
        );
        assert_eq!(
            chosen.metrics().get(0).unwrap().bearing_x(),
            Fixed::from_ratio(-257, 256)
        );
        assert_eq!(
            chosen.metrics().get(0).unwrap().bearing_y(),
            Fixed::from_ratio(design * 256 + 511, 256)
        );
        assert_eq!(chosen.map().len(), chosen.metrics().len());
        if index < 2 {
            assert_eq!(chosen.map().get(1), Some(Region::new(0, 8, 8, 8).unwrap()));
        } else {
            assert_eq!(chosen.map().get(0), Some(Region::new(1, 2, 3, 4).unwrap()));
            assert_eq!(chosen.map().get(1), Some(Region::new(8, 8, 0, 0).unwrap()));
        }
    }
    let nearest = face
        .select(
            FontRepresentationRequest::new(512).with_fallback(FontRepresentationFallback::Nearest),
        )
        .unwrap();
    assert_eq!(nearest.index(), 3);
    assert!(nearest.used_fallback());
    assert!(face.select(FontRepresentationRequest::new(512)).is_err());
    assert!(face.get(4).is_none());
    assert!(face.get(usize::MAX).is_none());
    // Bound values borrow source tables, not the stack-local face wrapper.
    let selected = {
        let local = fixture.bind().unwrap();
        local.get(2).unwrap()
    };
    assert_eq!(selected.surface().packing(), GlyphPacking::Atlas2D);
    assert_eq!(
        selected.metrics().as_bytes().as_ptr(),
        fixture.metrics[72..].as_ptr()
    );
}

#[test]
fn cardinality_and_exact_table_boundaries_precede_record_access() {
    let fixture = Fixture::new();
    let codepoints = FontCodepoints::open(&fixture.codepoints).unwrap();
    assert!(matches!(
        FaceTables::new(
            FontCodepoints::open(&fixture.codepoints[..4]).unwrap(),
            fixture.table(),
            &fixture.metrics,
            &fixture.maps
        ),
        Err(FaceTablesError::Cardinality {
            codepoints: 1,
            glyphs: 2
        })
    ));
    for size in 0..fixture.metrics.len() {
        assert!(matches!(
            FaceTables::new(
                codepoints,
                fixture.table(),
                &fixture.metrics[..size],
                &fixture.maps
            ),
            Err(FaceTablesError::MetricsLength { expected: 144, .. })
        ));
    }
    for size in 1..32 {
        assert!(matches!(
            FaceTables::new(
                codepoints,
                fixture.table(),
                &fixture.metrics,
                &fixture.maps[..size]
            ),
            Err(FaceTablesError::MapsLength { table_len: 32, .. })
        ));
    }
    assert!(matches!(
        FaceTables::new(codepoints, fixture.table(), &fixture.metrics, &[]),
        Err(FaceTablesError::MapOutOfBounds { representation: 2 })
    ));
    let mut invalid = Fixture::new();
    invalid.metrics[2 * 36 + 8..2 * 36 + 12].fill(0);
    assert!(matches!(
        invalid.bind(),
        Err(FaceTablesError::Metrics {
            representation: 2,
            ..
        })
    ));
}

#[test]
fn map_aliases_are_complete_referenced_tables_with_per_surface_bounds() {
    let mut fixture = Fixture::new();
    fixture.records[12..16].copy_from_slice(&32u32.to_le_bytes());
    assert!(matches!(
        fixture.bind(),
        Err(FaceTablesError::ImplicitMapOffset {
            representation: 0,
            offset: 32
        })
    ));
    fixture.records[12..16].fill(0);
    for offset in [1, 16, 31, u32::MAX] {
        fixture.records[44..48].copy_from_slice(&offset.to_le_bytes());
        assert!(matches!(
            fixture.bind(),
            Err(FaceTablesError::MapOffset {
                representation: 2,
                ..
            })
        ));
    }
    fixture.records[44..48].copy_from_slice(&32u32.to_le_bytes());
    assert!(matches!(
        fixture.bind(),
        Err(FaceTablesError::MapOutOfBounds { representation: 2 })
    ));
    fixture.records[44..48].fill(0);
    let mut maps = [0; 64];
    maps[..32].copy_from_slice(&fixture.maps);
    maps[32..].copy_from_slice(&fixture.maps);
    let codepoints = FontCodepoints::open(&fixture.codepoints).unwrap();
    assert!(matches!(
        FaceTables::new(codepoints, fixture.table(), &fixture.metrics, &maps),
        Err(FaceTablesError::UnreferencedMaps)
    ));
    fixture.records[60..64].copy_from_slice(&32u32.to_le_bytes());
    let face = FaceTables::new(codepoints, fixture.table(), &fixture.metrics, &maps).unwrap();
    assert_eq!(
        face.get(2).unwrap().map().get(0),
        face.get(3).unwrap().map().get(0)
    );
    fixture.records[60..64].fill(0);
    let mut surfaces = [0; 72];
    surfaces[..48].copy_from_slice(&fixture.surfaces);
    GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::Atlas2D, 3, 8, 0)
        .unwrap()
        .encode_record_into(&mut surfaces[48..])
        .unwrap();
    fixture.records[58..60].copy_from_slice(&2u16.to_le_bytes());
    let table = RepresentationTable::open(&fixture.records, &surfaces, 2, &PayloadLimits::EMBEDDED)
        .unwrap();
    assert!(matches!(
        FaceTables::new(codepoints, table, &fixture.metrics, &fixture.maps),
        Err(FaceTablesError::Map {
            representation: 3,
            ..
        })
    ));
    fixture.records[58..60].copy_from_slice(&1u16.to_le_bytes());
    fixture.maps[0..4].copy_from_slice(&8u32.to_le_bytes());
    assert!(matches!(
        fixture.bind(),
        Err(FaceTablesError::Map {
            representation: 2,
            ..
        })
    ));
}

#[test]
fn empty_glyph_tables_and_unaligned_wire_bytes_remain_borrowed() {
    let fixture = Fixture::new();
    let mut metrics = [0; 48];
    for index in 0..4 {
        metrics[index * 12..(index + 1) * 12]
            .copy_from_slice(&fixture.metrics[index * 36..index * 36 + 12]);
    }
    let table = RepresentationTable::open(
        &fixture.records,
        &fixture.surfaces,
        0,
        &PayloadLimits::EMBEDDED,
    )
    .unwrap();
    let empty = FaceTables::new(FontCodepoints::open(&[]).unwrap(), table, &metrics, &[]).unwrap();
    assert_eq!(empty.glyph_count(), 0);
    for index in 0..4 {
        assert!(empty.get(index).unwrap().metrics().is_empty());
        assert!(empty.get(index).unwrap().map().is_empty());
    }
    assert!(FaceTables::new(FontCodepoints::open(&[]).unwrap(), table, &metrics, &[0]).is_err());
    let mut records = [0; 65];
    records[1..].copy_from_slice(&fixture.records);
    let mut surfaces = [0; 49];
    surfaces[1..].copy_from_slice(&fixture.surfaces);
    let mut metrics = [0; 145];
    metrics[1..].copy_from_slice(&fixture.metrics);
    let mut maps = [0; 33];
    maps[1..].copy_from_slice(&fixture.maps);
    let table =
        RepresentationTable::open(&records[1..], &surfaces[1..], 2, &PayloadLimits::EMBEDDED)
            .unwrap();
    let face = FaceTables::new(
        FontCodepoints::open(&fixture.codepoints).unwrap(),
        table,
        &metrics[1..],
        &maps[1..],
    )
    .unwrap();
    assert_eq!(
        face.get(2).unwrap().map().get(0),
        fixture.bind().unwrap().get(2).unwrap().map().get(0)
    );
}
