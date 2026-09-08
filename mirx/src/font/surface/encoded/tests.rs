use super::*;
use crate::image::AtlasMap;
use crate::{
    coding::{CodingId, Lz4, Rle},
    font::{GlyphPacking, RawGlyphs},
    image::{EncodedImageAsset, Region, SampleLayout, UnitGroupRecord},
    media::{CodingRecord, DataIntegrity, MediaSectionKind},
};

#[repr(align(64))]
struct Buffer([u8; 256]);

fn bind<'map, 'data>(
    bytes: &'data [u8],
    map: GlyphMap<'map>,
    layout: SampleLayout,
    packing: GlyphPacking,
    width: u32,
    height: u32,
) -> EncodedGlyphs<'map, 'data> {
    let media = MediaPayload::open(bytes).unwrap();
    let ordinal = |kind| {
        media
            .sections()
            .position(|section| section.descriptor().kind() == kind)
            .map(|index| index as u16)
    };
    let mut record = GlyphSurfaceRecord::new(
        layout,
        packing,
        width,
        height,
        ordinal(MediaSectionKind::DATA).unwrap(),
    )
    .unwrap()
    .with_codings(ordinal(MediaSectionKind::CODINGS).unwrap())
    .unwrap();
    if let Some(groups) = ordinal(MediaSectionKind::UNIT_GROUPS) {
        record = record
            .with_groups(groups, ordinal(MediaSectionKind::UNIT_INDEX))
            .unwrap();
    }
    record.encoded_glyphs(media, map).unwrap()
}

#[test]
fn independent_glyph_units_share_scalar_plans_and_aligned_error_atomic_output() {
    let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
    for (coding, data) in [
        (Rle::new().record(), &[0x83, 1, 0x83, 2][..]),
        (
            Lz4::new().record(),
            &[0x40, 1, 1, 1, 1, 0x40, 2, 2, 2, 2][..],
        ),
    ] {
        let codings = [coding];
        let records = [UnitGroupRecord::new(0, 0..data.len() as u32)
            .unwrap()
            .with_tiles(2, 2)];
        let bytes = EncodedImageAsset::from_records(surface, &codings, &records, data)
            .encode()
            .unwrap();
        let glyphs = bind(
            &bytes,
            GlyphMap::cells(2, 2, 2).unwrap(),
            SampleLayout::A8,
            GlyphPacking::GlyphMajor,
            2,
            2,
        );
        assert_eq!(glyphs.len(), 2);
        assert!(!glyphs.is_empty());
        assert_eq!(glyphs.group_count(), 1);
        assert_eq!(
            glyphs.input_alignment().map(crate::ByteAlignment::get),
            Ok(1)
        );
        glyphs.preflight(&PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(
            glyphs.preflight(&PayloadLimits::EMBEDDED.with_max_font_glyphs(1)),
            Err(EncodedGlyphError::TooManyGlyphs {
                limit: 1,
                actual: 2
            })
        );
        assert!(
            glyphs
                .groups_into(&mut [], &mut CoverageBudget::new(1000))
                .is_err()
        );
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        assert_eq!(groups.len(), 2);
        assert!(!groups.is_empty());
        assert_eq!(groups.group_count(), 1);
        let requirements = SurfaceRequirements::new()
            .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
            .with_stride_multiple(64);
        for (index, value) in [(0, 1), (1, 2)] {
            let plan = groups
                .decode_plan(index, requirements, &PayloadLimits::EMBEDDED)
                .unwrap();
            assert_eq!(plan.unit_count(), 1);
            assert_eq!(plan.input_byte_len(), (data.len() / 2) as u64);
            assert_eq!(plan.workspace_requirements().byte_len(), 4);
            assert_eq!(plan.memory_plan().byte_len(), 128);
            let mut output = Buffer([0xa5; 256]);
            let mut workspace = [0x5a; 8];
            assert!(matches!(
                plan.decode_into(&mut output.0, &mut workspace[..3]),
                Err(DecodeError::Workspace(_))
            ));
            assert!(matches!(
                plan.decode_into(&mut output.0[1..], &mut workspace),
                Err(DecodeError::Output(_))
            ));
            assert!(matches!(
                plan.decode_into(&mut output.0[..127], &mut workspace),
                Err(DecodeError::Output(_))
            ));
            assert_eq!(output.0, [0xa5; 256]);
            assert_eq!(workspace, [0x5a; 8]);
            let decoded = plan.decode_into(&mut output.0, &mut workspace).unwrap();
            for row in decoded.plane(0).unwrap().rows().unwrap() {
                assert_eq!(row, &[value; 2]);
            }
            assert_eq!(&output.0[2..64], &[0; 62]);
            assert_eq!(&output.0[66..128], &[0; 62]);
            assert_eq!(&output.0[128..], &[0xa5; 128]);
            assert_eq!(&workspace[4..], &[0x5a; 4]);
        }
        assert!(matches!(
            groups.decode_plan(2, requirements, &PayloadLimits::EMBEDDED),
            Err(EncodedGlyphError::GlyphOutOfBounds { index: 2, count: 2 })
        ));
    }
}

#[test]
fn implicit_atlas_streams_preserve_packed_regions_and_release_map_metadata() {
    for layout in [
        SampleLayout::A1,
        SampleLayout::A2,
        SampleLayout::A4,
        SampleLayout::A8,
    ] {
        let surface = SurfaceDescriptor::new(9, 3, layout, ColorDescription::NONE).unwrap();
        let len = layout
            .plane_geometry(9, 3, 0)
            .unwrap()
            .minimum_stride()
            .unwrap() as usize
            * 3;
        let raw = [0x5a; 27];
        let mut stream = [0; 64];
        let encoded_len = Rle::new().encode_into(&raw[..len], &mut stream).unwrap();
        let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &stream[..encoded_len])
            .encode()
            .unwrap();
        let mut slots = [None];
        let mut expected = [0xa5; 32];
        let plan = {
            let regions = [
                Region::new(1, 1, 5, 2).unwrap(),
                Region::new(0, 0, 0, 0).unwrap(),
            ];
            let map = GlyphMap::atlas(AtlasMap::new(9, 3, &regions).unwrap());
            let glyphs = bind(&bytes, map, layout, GlyphPacking::Atlas2D, 9, 3);
            let raw_glyphs = RawGlyphs::builder(map, layout).build(&raw[..len]).unwrap();
            let glyph = raw_glyphs.get(0).unwrap();
            let raw_plan = glyph.memory_plan(SurfaceRequirements::new()).unwrap();
            glyph.copy_into(&mut expected, raw_plan).unwrap();
            let groups = glyphs
                .groups_into(&mut slots, &mut CoverageBudget::new(1000))
                .unwrap();
            let empty = groups
                .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                .unwrap();
            assert_eq!(empty.unit_count(), 0);
            assert_eq!(empty.input_byte_len(), 0);
            assert_eq!(empty.checksum_byte_len(), 0);
            assert_eq!(empty.workspace_requirements().byte_len(), 0);
            empty.decode_into(&mut [], &mut []).unwrap();
            groups
                .decode_plan(0, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                .unwrap()
        };
        assert_eq!(plan.workspace_requirements().byte_len(), len);
        let mut output = [0xa5; 32];
        let mut workspace = [0; 27];
        plan.decode_into(&mut output, &mut workspace).unwrap();
        assert_eq!(output, expected);
    }
}

#[test]
fn selected_glyph_integrity_respects_partition_scope_and_empty_requests() {
    let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..4).unwrap().with_tiles(2, 2)];
    for integrity in [DataIntegrity::Whole, DataIntegrity::Indexed(&[2, 4])] {
        let mut bytes =
            EncodedImageAsset::from_records(surface, &codings, &records, &[0x83, 1, 0x83, 2])
                .with_integrity(integrity)
                .encode()
                .unwrap();
        let start = MediaPayload::open(&bytes)
            .unwrap()
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset() as usize;
        bytes[start + 3] ^= 1;
        let regions = [
            Region::new(0, 0, 2, 2).unwrap(),
            Region::new(0, 2, 2, 2).unwrap(),
            Region::new(0, 0, 0, 0).unwrap(),
        ];
        let glyphs = bind(
            &bytes,
            GlyphMap::atlas(AtlasMap::new(2, 4, &regions).unwrap()),
            SampleLayout::A8,
            GlyphPacking::Atlas2D,
            2,
            4,
        );
        assert!(glyphs.preflight(&PayloadLimits::EMBEDDED).is_err());
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let first = groups.decode_plan(0, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED);
        assert_eq!(first.is_ok(), integrity != DataIntegrity::Whole);
        if let Ok(plan) = first {
            assert_eq!(plan.checksum_byte_len(), 2);
            let mut output = [0; 4];
            plan.decode_into(&mut output, &mut [0; 4]).unwrap();
            assert_eq!(output, [1; 4]);
        }
        assert!(
            groups
                .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                .is_err()
        );
        let empty = groups
            .decode_plan(2, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
            .unwrap();
        assert_eq!(empty.checksum_byte_len(), 0);
    }
}

#[test]
fn metadata_admission_does_not_imply_codec_support_or_valid_file_placement() {
    let surface = SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let map = GlyphMap::cells(2, 2, 1).unwrap();
    for coding in [
        CodingRecord::new(CodingId::new(500), 1, &[]),
        CodingRecord::new(CodingId::PIXEL, 1, &[]),
    ] {
        let bytes = EncodedImageAsset::new(surface, coding, &[0x83, 1])
            .encode()
            .unwrap();
        let glyphs = bind(
            &bytes,
            map,
            SampleLayout::A8,
            GlyphPacking::GlyphMajor,
            2,
            2,
        );
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        assert!(glyphs.preflight(&PayloadLimits::EMBEDDED).is_err());
        assert!(
            groups
                .decode_plan(0, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                .is_err()
        );
    }
    let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 1])
        .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
        .encode()
        .unwrap();
    let glyphs = bind(
        &bytes,
        map,
        SampleLayout::A8,
        GlyphPacking::GlyphMajor,
        2,
        2,
    );
    assert_eq!(
        glyphs.input_alignment().map(crate::ByteAlignment::get),
        Ok(64)
    );
    for offset in [0, 64, 128] {
        glyphs
            .with_file_offset(offset)
            .preflight(&PayloadLimits::EMBEDDED)
            .unwrap();
    }
    for offset in [1, 63, u32::MAX] {
        assert!(
            glyphs
                .with_file_offset(offset)
                .preflight(&PayloadLimits::EMBEDDED)
                .is_err()
        );
        assert!(
            glyphs
                .with_file_offset(offset)
                .groups_into(&mut [None], &mut CoverageBudget::new(1000))
                .is_err()
        );
    }
    let media = MediaPayload::open(&bytes).unwrap();
    let raw = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 2, 3).unwrap();
    assert!(matches!(
        raw.encoded_glyphs(media, map),
        Err(EncodedGlyphError::ExpectedEncodedStorage)
    ));
    let record = raw.with_codings(1).unwrap().with_groups(2, None).unwrap();
    assert!(matches!(
        record.encoded_glyphs(media, GlyphMap::cells(1, 1, 4).unwrap()),
        Err(EncodedGlyphError::Record(
            GlyphSurfaceRecordError::RasterMapMismatch
        ))
    ));
    assert!(
        record
            .with_codings(0)
            .unwrap()
            .encoded_glyphs(media, map)
            .is_err()
    );
}
