use super::*;
use crate::{
    font::{
        FontRepresentation, FontRepresentationFallback, FontRepresentationPreference, GlyphPacking,
    },
    image::SampleLayout,
};

fn surfaces() -> [u8; 48] {
    let mut bytes = [0; 48];
    for (index, (layout, packing, width, height)) in [
        (SampleLayout::A4, GlyphPacking::GlyphMajor, 8, 8),
        (SampleLayout::A8, GlyphPacking::Atlas2D, 8, 8),
    ]
    .into_iter()
    .enumerate()
    {
        let record = GlyphSurfaceRecord::new(layout, packing, width, height, 0)
            .unwrap()
            .encode_record();
        bytes[index * 24..(index + 1) * 24].copy_from_slice(&record);
    }
    bytes
}

fn representations() -> [FontRepresentation; 4] {
    [
        FontRepresentation::coverage(4, 12, 64).unwrap(),
        FontRepresentation::coverage(4, 16, 64).unwrap(),
        FontRepresentation::signed_distance(8, 3, 24, 17, 48, 64).unwrap(),
        FontRepresentation::signed_distance(8, 5, 64, 49, 128, 64).unwrap(),
    ]
}

fn records() -> [u8; REPRESENTATION_RECORD_LEN * 4] {
    let mut bytes = [0; REPRESENTATION_RECORD_LEN * 4];
    for (index, metadata) in representations().into_iter().enumerate() {
        let record = RepresentationRecord::new(metadata, u16::from(index > 1))
            .with_atlas_map_range(if index > 1 { 2 } else { 0 }, if index > 1 { 2 } else { 0 })
            .encode_record()
            .unwrap();
        bytes[index * REPRESENTATION_RECORD_LEN..(index + 1) * REPRESENTATION_RECORD_LEN]
            .copy_from_slice(&record);
    }
    bytes
}

#[test]
fn native_and_wire_tables_share_identity_and_selection_semantics() {
    let surfaces = surfaces();
    let records = records();
    let table =
        RepresentationTable::open(&records, &surfaces, 2, &PayloadLimits::EMBEDDED).unwrap();
    let metadata = representations();
    let native = FontRepresentations::new(&metadata).unwrap();
    assert_eq!(table.glyph_count(), 2);
    assert_eq!(table.len(), 4);
    assert!(!table.is_empty());
    for ppem in [
        0,
        1,
        12,
        16,
        17,
        24,
        40,
        48,
        49,
        64,
        120,
        128,
        129,
        u16::MAX,
    ] {
        for preference in [
            FontRepresentationPreference::Auto,
            FontRepresentationPreference::Coverage,
            FontRepresentationPreference::SignedDistance,
        ] {
            for fallback in [
                FontRepresentationFallback::Reject,
                FontRepresentationFallback::Nearest,
            ] {
                let request = FontRepresentationRequest::new(ppem)
                    .with_preference(preference)
                    .with_fallback(fallback);
                assert_eq!(table.select(request), native.select(request));
            }
        }
    }
    for (index, expected) in metadata.into_iter().enumerate() {
        assert_eq!(table.get(index).unwrap().representation(), expected);
    }
    let mut duplicate = records;
    duplicate[REPRESENTATION_RECORD_LEN..REPRESENTATION_RECORD_LEN * 2]
        .copy_from_slice(&records[..REPRESENTATION_RECORD_LEN]);
    // Different storage references do not make the same semantic representation unique.
    duplicate[REPRESENTATION_RECORD_LEN + 12] = 17;
    duplicate[REPRESENTATION_RECORD_LEN + 16] = 1;
    assert!(matches!(
        RepresentationTable::open(&duplicate, &surfaces, 2, &PayloadLimits::EMBEDDED),
        Err(RepresentationTableError::Selection(
            FontSelectionError::DuplicateRepresentation {
                first: 0,
                duplicate: 1
            }
        ))
    ));
}

#[test]
fn scalar_application_classes_keep_full_identifiers_and_explicit_selection() {
    let surfaces = surfaces();
    for kind in [0, 1, u16::MAX] {
        let metadata = FontRepresentation::application(kind, 20, 10, 40, 64).unwrap();
        let records = RepresentationRecord::new(metadata, 0)
            .encode_record()
            .unwrap();
        let table =
            RepresentationTable::open(&records, &surfaces, 2, &PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(table.get(0).unwrap().representation(), metadata);
        assert_eq!(
            table.select(FontRepresentationRequest::new(20)),
            Err(FontSelectionError::NoMatch)
        );
        let selected = table
            .select(
                FontRepresentationRequest::new(20)
                    .with_preference(FontRepresentationPreference::Application(kind)),
            )
            .unwrap();
        assert_eq!(selected.representation(), metadata);
    }
}

#[test]
fn limits_precede_record_interpretation_and_partial_tables_never_open() {
    let surfaces = surfaces();
    let mut records = records();
    records[0] = 255;
    assert!(matches!(
        RepresentationTable::open(
            &records,
            &surfaces,
            2,
            &PayloadLimits::EMBEDDED.with_max_font_representations(3)
        ),
        Err(RepresentationTableError::TooManyRepresentations { actual: 4, .. })
    ));
    assert!(matches!(
        RepresentationTable::open(
            &records,
            &surfaces,
            2,
            &PayloadLimits::EMBEDDED.with_max_font_glyphs(1)
        ),
        Err(RepresentationTableError::TooManyGlyphs { actual: 2, .. })
    ));
    assert!(matches!(
        RepresentationTable::open(
            &records[..REPRESENTATION_RECORD_LEN],
            &surfaces,
            2,
            &PayloadLimits::EMBEDDED.with_max_font_representations(0)
        ),
        Err(RepresentationTableError::TooManyRepresentations { .. })
    ));
    for length in 1..REPRESENTATION_RECORD_LEN {
        assert!(matches!(
            RepresentationTable::open(&records[..length], &surfaces, 2, &PayloadLimits::HOST),
            Err(RepresentationTableError::PartialRecords {
                kind: MediaSectionKind::REPRESENTATIONS,
                ..
            })
        ));
    }
    for length in 1..24 {
        assert!(matches!(
            RepresentationTable::open(&records, &surfaces[..length], 2, &PayloadLimits::HOST),
            Err(RepresentationTableError::PartialRecords {
                kind: MediaSectionKind::SURFACE_GROUPS,
                ..
            })
        ));
    }
    assert!(matches!(
        RepresentationTable::open(&records, &surfaces, 2, &PayloadLimits::HOST),
        Err(RepresentationTableError::Record { index: 0, .. })
    ));
    assert!(matches!(
        RepresentationTable::open(&[], &[], 0, &PayloadLimits::EMBEDDED),
        Err(RepresentationTableError::Selection(
            FontSelectionError::Empty
        ))
    ));
}

#[test]
fn surface_ordinals_geometry_and_derived_costs_have_one_authority() {
    let bytes =
        RepresentationRecord::new(FontRepresentation::coverage(4, 16, 999).unwrap(), u16::MAX)
            .encode_record()
            .unwrap();
    let mut surfaces = alloc::vec![0xff; (usize::from(u16::MAX) + 1) * GLYPH_SURFACE_RECORD_LEN];
    let last = surfaces.len() - GLYPH_SURFACE_RECORD_LEN;
    let surface = GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 3, 2, 0)
        .unwrap()
        .encode_record();
    surfaces[last..].copy_from_slice(&surface);
    let table = RepresentationTable::open(
        &bytes,
        &surfaces,
        3,
        &PayloadLimits::EMBEDDED.with_max_decoded_bytes(0),
    )
    .unwrap();
    assert_eq!(table.get(0).unwrap().representation().decoded_bytes(), 12);
    assert_eq!(table.get(0).unwrap().surface_index(), u16::MAX);
    assert!(matches!(
        RepresentationTable::open(&bytes, &surfaces[..last], 3, &PayloadLimits::EMBEDDED),
        Err(RepresentationTableError::SurfaceOutOfBounds {
            surface: u16::MAX,
            ..
        })
    ));
    surfaces.extend_from_slice(&[0; 24]);
    assert!(matches!(
        RepresentationTable::open(&bytes, &surfaces, 3, &PayloadLimits::HOST),
        Err(RepresentationTableError::TooManySurfaces { actual: 65537 })
    ));
    let record = RepresentationRecord::new(FontRepresentation::coverage(4, 16, 0).unwrap(), 0)
        .encode_record()
        .unwrap();
    for layout in [
        SampleLayout::RGB888,
        SampleLayout::NV12,
        SampleLayout::new(0xf001),
    ] {
        let surface = GlyphSurfaceRecord::new(layout, GlyphPacking::GlyphMajor, 1, 1, 0)
            .unwrap()
            .encode_record();
        assert!(matches!(
            RepresentationTable::open(&record, &surface, 1, &PayloadLimits::HOST),
            Err(RepresentationTableError::Surface {
                error: GlyphSurfaceRecordError::UnsupportedLayout(_),
                ..
            })
        ));
    }
    let surface =
        GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 1, u32::MAX, 0)
            .unwrap()
            .encode_record();
    assert!(matches!(
        RepresentationTable::open(&record, &surface, 2, &PayloadLimits::HOST),
        Err(RepresentationTableError::Surface {
            error: GlyphSurfaceRecordError::CellMap(_),
            ..
        })
    ));
    let table = RepresentationTable::open(&record, &surface, 0, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(table.get(0).unwrap().representation().decoded_bytes(), 0);
}

#[test]
fn unaligned_iteration_and_selected_values_do_not_retain_source_lifetimes() {
    let (selected, record) = {
        let mut record_bytes = [0xff; REPRESENTATION_RECORD_LEN * 4 + 1];
        record_bytes[1..].copy_from_slice(&records());
        let mut surface_bytes = [0xff; 49];
        surface_bytes[1..].copy_from_slice(&surfaces());
        let table = RepresentationTable::open(
            &record_bytes[1..],
            &surface_bytes[1..],
            2,
            &PayloadLimits::EMBEDDED,
        )
        .unwrap();
        let mut iter = table.iter();
        assert_eq!(iter.size_hint(), (4, Some(4)));
        assert_eq!(iter.nth(1), table.get(1));
        assert_eq!(iter.next_back(), table.get(3));
        assert_eq!(iter.next(), table.get(2));
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.nth(usize::MAX), None);
        assert_eq!(table.iter().nth_back(2), table.get(1));
        assert_eq!(table.iter().last(), table.get(3));
        assert_eq!(table.iter().count(), 4);
        let mut exhausted = table.iter();
        assert_eq!(exhausted.nth_back(usize::MAX), None);
        assert_eq!(exhausted.len(), 0);
        assert_eq!(table.get(usize::MAX), None);
        (
            table.select(FontRepresentationRequest::new(24)).unwrap(),
            table.get(2).unwrap(),
        )
    };
    assert_eq!(selected.representation(), record.representation());
    assert_eq!(selected.index(), 2);
}
