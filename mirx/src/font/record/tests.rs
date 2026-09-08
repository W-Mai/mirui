use super::*;

fn surface(layout: SampleLayout) -> SurfaceDescriptor {
    SurfaceDescriptor::new(
        5,
        3,
        layout,
        if layout.is_alpha() {
            crate::image::ColorDescription::NONE
        } else {
            crate::image::ColorDescription::SRGB
        },
    )
    .unwrap()
}

#[test]
fn independent_bytes_keep_size_defaults_and_references_explicit() {
    let surface = surface(SampleLayout::A4);
    let coverage =
        RepresentationRecord::new(FontRepresentation::coverage(4, 16, 9).unwrap(), 0x1234)
            .with_atlas_map_range(0x07654321, 0x10203040);
    let sdf = RepresentationRecord::new(
        FontRepresentation::signed_distance(4, 3, 24, 17, 48, 9).unwrap(),
        2,
    )
    .with_atlas_map_range(2, 4);
    let app = RepresentationRecord::new(
        FontRepresentation::application(1, 20, 12, 48, 9).unwrap(),
        4,
    );
    for (record, bytes) in [
        (
            coverage,
            [
                0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0x34, 0x12, 0x21, 0x43, 0x65, 0x07, 0x40, 0x30,
                0x20, 0x10,
            ],
        ),
        (
            sdf,
            [
                1, 0, 24, 0, 17, 0, 48, 0, 3, 0, 2, 0, 2, 0, 0, 0, 4, 0, 0, 0,
            ],
        ),
        (
            app,
            [
                2, 0, 20, 0, 12, 0, 48, 0, 1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ),
    ] {
        assert_eq!(
            RepresentationRecord::from_record(&bytes, surface),
            Ok(record)
        );
        assert_eq!(record.encode_record().unwrap(), bytes);
        let mut unaligned = [0xa5; REPRESENTATION_RECORD_LEN + 1];
        unaligned[1..].copy_from_slice(&bytes);
        assert_eq!(
            RepresentationRecord::from_record(&unaligned[1..], surface),
            Ok(record)
        );
        assert_eq!(record.validate_for(surface), Ok(()));
    }
    assert_eq!(coverage.surface_index(), 0x1234);
    assert_eq!(coverage.atlas_map_offset(), 0x07654321);
    assert_eq!(coverage.atlas_map_count(), 0x10203040);
    assert_eq!(coverage.representation().min_ppem(), 16);
    assert_eq!(coverage.representation().max_ppem(), 16);
}

#[test]
fn sample_depth_and_cost_come_only_from_the_bound_surface() {
    let bytes = [0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    for (layout, bits, size) in [
        (SampleLayout::A1, 1, 3),
        (SampleLayout::A2, 2, 6),
        (SampleLayout::A4, 4, 9),
        (SampleLayout::A8, 8, 15),
    ] {
        let surface = surface(layout);
        let record = RepresentationRecord::from_record(&bytes, surface).unwrap();
        assert_eq!(
            record.representation().kind(),
            FontRepresentationKind::Coverage { bits }
        );
        assert_eq!(record.representation().decoded_bytes(), size);
        let encoded = record.encode_record().unwrap();
        assert_eq!(encoded, bytes);
    }
    let native = RepresentationRecord::new(FontRepresentation::coverage(4, 16, 9).unwrap(), 0);
    assert_eq!(
        native.validate_for(surface(SampleLayout::A8)),
        Err(RepresentationRecordError::SampleDepthMismatch {
            expected: 8,
            actual: 4
        })
    );
    let stale = RepresentationRecord::new(FontRepresentation::coverage(4, 16, 16).unwrap(), 0);
    assert_eq!(
        stale.validate_for(surface(SampleLayout::A4)),
        Err(RepresentationRecordError::DecodedSizeMismatch {
            expected: 9,
            actual: 16
        })
    );
    assert_eq!(
        native.validate_for(surface(SampleLayout::RGBA8888)),
        Err(RepresentationRecordError::UnsupportedLayout(
            SampleLayout::RGBA8888
        ))
    );
    let mut sdf = bytes;
    sdf[0] = 1;
    sdf[4] = 12;
    sdf[6] = 48;
    sdf[8] = 2;
    for layout in [SampleLayout::A1, SampleLayout::A2] {
        assert!(matches!(
            RepresentationRecord::from_record(&sdf, surface(layout)),
            Err(RepresentationRecordError::Representation(
                FontRepresentationError::InvalidBits { .. }
            ))
        ));
    }
    for layout in [SampleLayout::A4, SampleLayout::A8] {
        assert!(RepresentationRecord::from_record(&sdf, surface(layout)).is_ok());
    }
}

#[test]
fn malformed_class_size_and_reserved_fields_are_rejected_before_access() {
    let surface = surface(SampleLayout::A8);
    let valid = [0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    for offset in 4..10 {
        let mut bytes = valid;
        bytes[offset] = 1;
        assert_eq!(
            RepresentationRecord::from_record(&bytes, surface),
            Err(RepresentationRecordError::NonCanonicalCoverage)
        );
    }
    for class in 3..=255 {
        let mut bytes = valid;
        bytes[0] = class;
        assert_eq!(
            RepresentationRecord::from_record(&bytes, surface),
            Err(RepresentationRecordError::UnknownClass(class))
        );
    }
    let mut reserved = valid;
    reserved[1] = 1;
    assert_eq!(
        RepresentationRecord::from_record(&reserved, surface),
        Err(RepresentationRecordError::ReservedNonZero { offset: 1 })
    );
    let mut zero_size = valid;
    zero_size[2] = 0;
    assert!(matches!(
        RepresentationRecord::from_record(&zero_size, surface),
        Err(RepresentationRecordError::Representation(
            FontRepresentationError::InvalidDesignPpem
        ))
    ));
    for (min, max, spread) in [(0, 48, 1), (20, 12, 1), (17, 48, 1), (1, 15, 1), (1, 48, 0)] {
        let mut bytes = valid;
        bytes[0] = 1;
        bytes[4] = min;
        bytes[6] = max;
        bytes[8] = spread;
        assert!(RepresentationRecord::from_record(&bytes, surface).is_err());
    }
    for len in 0..REPRESENTATION_RECORD_LEN {
        assert_eq!(
            RepresentationRecord::from_record(&valid[..len], surface),
            Err(RepresentationRecordError::Truncated {
                needed: REPRESENTATION_RECORD_LEN,
                available: len
            })
        );
    }

    let mut empty_range = valid;
    empty_range[12] = 1;
    assert_eq!(
        RepresentationRecord::from_record(&empty_range, surface),
        Err(RepresentationRecordError::EmptyAtlasMapRange { offset: 1 })
    );
    let mut overflowing_range = valid;
    overflowing_range[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    overflowing_range[16] = 2;
    assert_eq!(
        RepresentationRecord::from_record(&overflowing_range, surface),
        Err(RepresentationRecordError::AtlasMapRangeOverflow)
    );
    for record in [
        RepresentationRecord::new(FontRepresentation::coverage(8, 16, 15).unwrap(), 0)
            .with_atlas_map_range(1, 0),
        RepresentationRecord::new(FontRepresentation::coverage(8, 16, 15).unwrap(), 0)
            .with_atlas_map_range(u32::MAX, 2),
    ] {
        assert!(record.encode_record().is_err());
    }
}

#[test]
fn application_kinds_and_extreme_surface_sizes_remain_unambiguous() {
    for kind in [0, 1, 2, 0x8000, u16::MAX] {
        let surface = surface(SampleLayout::RGBA8888);
        let metadata = FontRepresentation::application(kind, 16, 1, u16::MAX, 60).unwrap();
        let record = RepresentationRecord::new(metadata, u16::MAX);
        let bytes = record.encode_record().unwrap();
        assert_eq!(bytes[0], 2);
        assert_eq!(&bytes[8..10], &kind.to_le_bytes());
        assert_eq!(record.validate_for(surface), Ok(()));
        assert_eq!(
            RepresentationRecord::from_record(&bytes, surface),
            Ok(record)
        );
    }
    let bytes = [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let empty = SurfaceDescriptor::new(
        0,
        u32::MAX,
        SampleLayout::A8,
        crate::image::ColorDescription::NONE,
    )
    .unwrap();
    assert_eq!(
        RepresentationRecord::from_record(&bytes, empty)
            .unwrap()
            .representation()
            .decoded_bytes(),
        0
    );
    let huge = SurfaceDescriptor::new(
        u32::MAX,
        2,
        SampleLayout::A8,
        crate::image::ColorDescription::NONE,
    )
    .unwrap();
    assert!(matches!(
        RepresentationRecord::from_record(&bytes, huge),
        Err(RepresentationRecordError::Memory(_))
    ));
}
