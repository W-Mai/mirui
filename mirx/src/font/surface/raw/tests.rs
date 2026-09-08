use super::*;
use crate::image::AtlasMap;
use crate::{
    image::{PlaneMemoryFlags, Region, SampleLayout},
    media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaSectionKind},
    wire::{write_u16_le, write_u32_le},
};

fn payload(sections: &[(MediaSectionKind, &[u8])]) -> alloc::vec::Vec<u8> {
    let mut offset = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * sections.len();
    let total =
        offset + sections.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() + MEDIA_CRC_LEN;
    let mut out = alloc::vec![0; total];
    out[0] = MEDIA_VERSION;
    write_u16_le(&mut out, 2, sections.len() as u16);
    for (index, (kind, bytes)) in sections.iter().enumerate() {
        let entry = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * index;
        write_u16_le(&mut out, entry, kind.raw());
        write_u16_le(&mut out, entry + 2, 1);
        write_u32_le(&mut out, entry + 4, offset as u32);
        write_u32_le(&mut out, entry + 8, bytes.len() as u32);
        out[offset..offset + bytes.len()].copy_from_slice(bytes);
        offset += bytes.len();
    }
    crate::media::refresh_checksums(&mut out);
    out
}

#[test]
fn repeated_data_sections_bind_exact_ordinals_without_checksumming_samples() {
    let mut bytes = payload(&[
        (MediaSectionKind::DATA, &[1, 2]),
        (MediaSectionKind::DATA, &[3, 4]),
    ]);
    let map = GlyphMap::cells(1, 1, 2).unwrap();
    let first =
        GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 0).unwrap();
    let second =
        GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 1).unwrap();
    let media = MediaPayload::open(&bytes).unwrap();
    let start = media.get(1).unwrap().descriptor().offset() as usize;
    for (record, index, expected) in [(first, 0, [1, 2]), (second, 1, [3, 4])] {
        let glyphs = record.raw_glyphs(media, map).unwrap();
        for (ordinal, byte) in expected.into_iter().enumerate() {
            let glyph = glyphs.get(ordinal).unwrap();
            let plane = glyph.storage().plane(0).unwrap();
            assert_eq!(plane.bytes(), &[byte]);
            assert_eq!(
                plane.bytes().as_ptr(),
                media.get(index).unwrap().bytes()[ordinal..].as_ptr()
            );
        }
    }
    bytes[start] ^= 1;
    let media = MediaPayload::open(&bytes).unwrap();
    second.raw_glyphs(media, map).unwrap();
    assert!(media.validate_data().is_err());
}

#[test]
fn one_physical_record_preserves_cell_gaps_atlas_regions_and_address_checks() {
    for layout in [
        SampleLayout::A1,
        SampleLayout::A2,
        SampleLayout::A4,
        SampleLayout::A8,
    ] {
        let plane = layout.plane_geometry(5, 3, 0).unwrap();
        let memory = PlaneMemoryLayout::builder(plane)
            .with_stride(16)
            .with_alignment(crate::ByteAlignment::new(64).unwrap())
            .with_data_offset(64)
            .build()
            .unwrap();
        let mut plane_bytes = [0; PLANE_RECORD_LEN];
        memory.encode_record_into(&mut plane_bytes).unwrap();
        // 64 prefix + 48 cell + 16 gap + 48 cell = 176 bytes.
        let data = [0x5a; 176];
        let bytes = payload(&[
            (MediaSectionKind::PLANES, &plane_bytes),
            (MediaSectionKind::DATA, &data),
        ]);
        let media = MediaPayload::open(&bytes).unwrap();
        let record = GlyphSurfaceRecord::new(layout, GlyphPacking::GlyphMajor, 5, 3, 1)
            .unwrap()
            .with_planes(0)
            .unwrap();
        let glyphs = record
            .raw_glyphs(media, GlyphMap::cells(5, 3, 2).unwrap())
            .unwrap();
        assert_eq!(glyphs.memory_layout(), memory);
        assert_eq!(glyphs.byte_len(), 176);
        assert!(glyphs.file_address_is_aligned(64));
        assert!(!glyphs.file_address_is_aligned(1));
        assert!(!glyphs.file_address_is_aligned(u32::MAX));
        assert_eq!(
            glyphs
                .get(1)
                .unwrap()
                .storage()
                .plane(0)
                .unwrap()
                .bytes()
                .as_ptr(),
            media.get(1).unwrap().bytes()[128..].as_ptr()
        );
        let regions = [
            Region::new(1, 1, 3, 2).unwrap(),
            Region::new(0, 0, 0, 0).unwrap(),
        ];
        let map = GlyphMap::atlas(AtlasMap::new(5, 3, &regions).unwrap());
        let bytes = payload(&[
            (MediaSectionKind::PLANES, &plane_bytes),
            (MediaSectionKind::DATA, &data[..112]),
        ]);
        let media = MediaPayload::open(&bytes).unwrap();
        let record = GlyphSurfaceRecord::new(layout, GlyphPacking::Atlas2D, 5, 3, 1)
            .unwrap()
            .with_planes(0)
            .unwrap();
        let glyphs = record.raw_glyphs(media, map).unwrap();
        assert_eq!(glyphs.get(0).unwrap().region(), regions[0]);
        assert_eq!(glyphs.get(1).unwrap().region(), regions[1]);
        assert_eq!(glyphs.byte_len(), 112);
    }
}

#[test]
fn mismatched_maps_plane_shapes_and_data_spans_cannot_bind() {
    let record =
        GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 1, 0).unwrap();
    let bytes = payload(&[(MediaSectionKind::DATA, &[1, 2, 3, 4])]);
    let media = MediaPayload::open(&bytes).unwrap();
    for map in [
        GlyphMap::cells(1, 2, 2).unwrap(),
        GlyphMap::atlas(AtlasMap::new(2, 1, &[]).unwrap()),
    ] {
        assert!(matches!(
            record.raw_glyphs(media, map),
            Err(GlyphSurfaceRecordError::RasterMapMismatch)
        ));
    }
    let map = GlyphMap::cells(2, 1, 2).unwrap();
    assert!(matches!(
        record.with_codings(1).unwrap().raw_glyphs(media, map),
        Err(GlyphSurfaceRecordError::ExpectedRawStorage)
    ));
    let unsupported =
        GlyphSurfaceRecord::new(SampleLayout::RGB888, GlyphPacking::GlyphMajor, 2, 1, 0).unwrap();
    assert!(matches!(
        unsupported.raw_glyphs(media, map),
        Err(GlyphSurfaceRecordError::UnsupportedLayout(_))
    ));
    assert!(matches!(
        record.raw_glyphs(media, GlyphMap::cells(2, 1, 1).unwrap()),
        Err(GlyphSurfaceRecordError::Storage(_))
    ));
    for length in [0, 23, 25, 48] {
        let plane_bytes = [0; 48];
        let bytes = payload(&[
            (MediaSectionKind::DATA, &[1, 2, 3, 4]),
            (MediaSectionKind::PLANES, &plane_bytes[..length]),
        ]);
        let media = MediaPayload::open(&bytes).unwrap();
        assert!(
            matches!(record.with_planes(1).unwrap().raw_glyphs(media, map), Err(GlyphSurfaceRecordError::PlaneRecordLength { actual }) if actual == length)
        );
    }
    let memory =
        PlaneMemoryLayout::tight(SampleLayout::A8.plane_geometry(2, 1, 0).unwrap()).unwrap();
    let mut plane_bytes = [0; 24];
    memory.encode_record_into(&mut plane_bytes).unwrap();
    plane_bytes[19] = 1;
    let bytes = payload(&[
        (MediaSectionKind::DATA, &[1, 2, 3, 4]),
        (MediaSectionKind::PLANES, &plane_bytes),
    ]);
    assert!(matches!(
        record
            .with_planes(1)
            .unwrap()
            .raw_glyphs(MediaPayload::open(&bytes).unwrap(), map),
        Err(GlyphSurfaceRecordError::Plane(_))
    ));
}

#[test]
fn empty_glyphs_and_unknown_physical_flags_retain_shared_storage_rules() {
    let empty = payload(&[(MediaSectionKind::DATA, &[])]);
    let media = MediaPayload::open(&empty).unwrap();
    let record =
        GlyphSurfaceRecord::new(SampleLayout::A1, GlyphPacking::GlyphMajor, 1, 1, 0).unwrap();
    assert!(
        record
            .raw_glyphs(media, GlyphMap::cells(1, 1, 0).unwrap())
            .unwrap()
            .is_empty()
    );
    let memory = PlaneMemoryLayout::builder(SampleLayout::A8.plane_geometry(1, 1, 0).unwrap())
        .with_flags(PlaneMemoryFlags::from_bits_retain(1))
        .build()
        .unwrap();
    let mut plane_bytes = [0; 24];
    memory.encode_record_into(&mut plane_bytes).unwrap();
    let bytes = payload(&[
        (MediaSectionKind::DATA, &[7]),
        (MediaSectionKind::PLANES, &plane_bytes),
    ]);
    let record = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 0)
        .unwrap()
        .with_planes(1)
        .unwrap();
    let glyphs = record
        .raw_glyphs(
            MediaPayload::open(&bytes).unwrap(),
            GlyphMap::cells(1, 1, 1).unwrap(),
        )
        .unwrap();
    assert_eq!(glyphs.memory_layout().flags().bits(), 1);
    assert!(
        glyphs
            .get(0)
            .unwrap()
            .storage()
            .plane(0)
            .unwrap()
            .row(0)
            .is_err()
    );
}
