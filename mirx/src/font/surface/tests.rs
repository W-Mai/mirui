use super::*;
use crate::image::{ColorDescription, EncodedImageAsset, RawImageAsset, SurfaceDescriptor};
use crate::media::{MEDIA_HEADER_LEN, MEDIA_SECTION_LEN};

fn record() -> GlyphSurfaceRecord {
    GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 12, 16, 7).unwrap()
}

#[test]
fn records_have_one_canonical_storage_state_and_exact_wire_layout() {
    let raw = record();
    let padded = raw.with_planes(8).unwrap();
    let encoded = raw.with_codings(9).unwrap();
    let grouped = encoded.with_groups(10, Some(11)).unwrap();
    for (record, references) in [
        (raw, [7, u16::MAX, u16::MAX, u16::MAX, u16::MAX]),
        (padded, [7, 8, u16::MAX, u16::MAX, u16::MAX]),
        (encoded, [7, u16::MAX, 9, u16::MAX, u16::MAX]),
        (grouped, [7, u16::MAX, 9, 10, 11]),
        (
            grouped.with_groups(12, None).unwrap(),
            [7, u16::MAX, 9, 12, u16::MAX],
        ),
        (grouped.with_codings(13).unwrap(), [7, u16::MAX, 13, 10, 11]),
    ] {
        let mut expected = [0; GLYPH_SURFACE_RECORD_LEN];
        expected[..2].copy_from_slice(&SampleLayout::A4.raw().to_le_bytes());
        expected[4] = 12;
        expected[8] = 16;
        for (index, value) in references.into_iter().enumerate() {
            expected[12 + index * 2..14 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut bytes = [0xcd; GLYPH_SURFACE_RECORD_LEN + 2];
        assert_eq!(record.encode_record_into(&mut bytes[1..]).unwrap(), 24);
        assert_eq!(&bytes[1..25], &expected);
        assert_eq!((bytes[0], bytes[25]), (0xcd, 0xcd));
        assert_eq!(
            GlyphSurfaceRecord::from_record(&bytes[1..]).unwrap(),
            record
        );
        assert_eq!(record.sample_layout(), SampleLayout::A4);
        assert_eq!((record.width(), record.height()), (12, 16));
        assert_eq!(record.packing(), GlyphPacking::GlyphMajor);
        for length in 0..GLYPH_SURFACE_RECORD_LEN {
            let mut output = [0xcd; GLYPH_SURFACE_RECORD_LEN];
            assert!(matches!(
                record.encode_record_into(&mut output[..length]),
                Err(GlyphSurfaceRecordError::BufferTooSmall { .. })
            ));
            assert_eq!(output, [0xcd; GLYPH_SURFACE_RECORD_LEN]);
            assert!(matches!(
                GlyphSurfaceRecord::from_record(&expected[..length]),
                Err(GlyphSurfaceRecordError::Truncated { .. })
            ));
        }
    }
}

#[test]
fn native_and_wire_paths_reject_conflicts_and_reserved_references() {
    let raw = record();
    assert_eq!(
        raw.with_planes(8).unwrap().with_codings(9),
        Err(GlyphSurfaceRecordError::ConflictingStorage)
    );
    assert_eq!(
        raw.with_codings(9).unwrap().with_planes(8),
        Err(GlyphSurfaceRecordError::ConflictingStorage)
    );
    assert_eq!(
        raw.with_groups(8, None),
        Err(GlyphSurfaceRecordError::MissingCodings)
    );
    assert!(raw.with_planes(u16::MAX).is_err());
    assert!(raw.with_codings(u16::MAX).is_err());
    assert!(
        raw.with_codings(9)
            .unwrap()
            .with_groups(u16::MAX, None)
            .is_err()
    );
    assert!(
        raw.with_codings(9)
            .unwrap()
            .with_groups(10, Some(u16::MAX))
            .is_err()
    );
    assert!(
        GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 1, 1, u16::MAX)
            .is_err()
    );
    let mut bytes = [0; 24];
    raw.encode_record_into(&mut bytes).unwrap();
    for mask in 0..16_u8 {
        let mut candidate = bytes;
        for field in 0..4 {
            if mask & (1 << field) != 0 {
                candidate[14 + field * 2..16 + field * 2].copy_from_slice(&8_u16.to_le_bytes());
            }
        }
        let planes = mask & 1 != 0;
        let codings = mask & 2 != 0;
        let groups = mask & 4 != 0;
        let index = mask & 8 != 0;
        let valid = !(planes && codings || groups && !codings || index && !groups);
        assert_eq!(
            GlyphSurfaceRecord::from_record(&candidate).is_ok(),
            valid,
            "mask {mask}"
        );
    }
    for offset in [3, 22, 23] {
        let mut candidate = bytes;
        candidate[offset] = 1;
        assert_eq!(
            GlyphSurfaceRecord::from_record(&candidate),
            Err(GlyphSurfaceRecordError::ReservedNonZero { offset })
        );
    }
    for packing in 2..=255 {
        let mut candidate = bytes;
        candidate[2] = packing;
        assert_eq!(
            GlyphSurfaceRecord::from_record(&candidate),
            Err(GlyphSurfaceRecordError::UnknownPacking(packing))
        );
    }
}

#[test]
fn logical_geometry_uses_shared_maps_without_repeating_sample_assumptions() {
    assert_eq!(record().logical_extent(3).unwrap(), (12, 48));
    assert_eq!(record().logical_extent(0).unwrap(), (12, 0));
    assert!(record().logical_extent(usize::MAX).is_err());
    for (width, height) in [(0, 0), (0, 1), (1, 0)] {
        assert!(
            GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, width, height, 0)
                .is_err()
        );
        let atlas =
            GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::Atlas2D, width, height, 0)
                .unwrap();
        assert_eq!(atlas.logical_extent(usize::MAX).unwrap(), (width, height));
    }
    let huge = GlyphSurfaceRecord::new(
        SampleLayout::new(0xffff),
        GlyphPacking::GlyphMajor,
        u32::MAX,
        u32::MAX,
        u16::MAX - 1,
    )
    .unwrap();
    assert_eq!(huge.logical_extent(1).unwrap(), (u32::MAX, u32::MAX));
    assert!(huge.logical_extent(2).is_err());
    let mut bytes = [0; 24];
    huge.encode_record_into(&mut bytes).unwrap();
    assert_eq!(GlyphSurfaceRecord::from_record(&bytes).unwrap(), huge);
}

#[test]
fn referenced_sections_require_exact_identity_not_body_or_sample_validation() {
    let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut bytes = RawImageAsset::new(surface, &[&[42]]).encode().unwrap();
    let raw = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 1).unwrap();
    let media = MediaPayload::open(&bytes).unwrap();
    raw.validate_sections(media).unwrap();
    assert!(matches!(
        raw.with_planes(0).unwrap().validate_sections(media),
        Err(GlyphSurfaceRecordError::SectionKind { index: 0, .. })
    ));
    assert_eq!(
        raw.with_planes(2).unwrap().validate_sections(media),
        Err(GlyphSurfaceRecordError::SectionOutOfBounds { index: 2 })
    );
    let data_offset = media.get(1).unwrap().descriptor().offset() as usize;
    bytes[data_offset] ^= 1;
    let media = MediaPayload::open(&bytes).unwrap();
    raw.validate_sections(media).unwrap();
    assert!(media.validate_data().is_err());
    for flags in [0_u16, 2, 3, u16::MAX] {
        let offset = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN + 2;
        bytes[offset..offset + 2].copy_from_slice(&flags.to_le_bytes());
        crate::media::refresh_checksums(&mut bytes);
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(
            raw.validate_sections(media),
            Err(GlyphSurfaceRecordError::SectionFlags {
                index: 1,
                flags: MediaSectionFlags::from_bits_retain(flags)
            })
        );
    }
    // Directory identity does not imply that a coding body is understood.
    let coding = crate::media::CodingRecord::new(crate::coding::CodingId::new(0xff00), 0, &[]);
    let bytes = EncodedImageAsset::new(surface, coding, &[17])
        .encode()
        .unwrap();
    let media = MediaPayload::open(&bytes).unwrap();
    let encoded = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::Atlas2D, 1, 1, 2)
        .unwrap()
        .with_codings(1)
        .unwrap();
    encoded.validate_sections(media).unwrap();
}
