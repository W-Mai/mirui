use super::*;
use crate::{
    coding::{CodingId, Rle},
    image::{
        ColorDescription, CoverageBudget, EncodedImageAsset, PlaneMemoryLayout, RawImageAsset,
        SampleLayout, SurfaceRequirements, UnitDecodeError,
        test_support::{data_offset, refresh_crc},
    },
    media::CodingRecord,
    wire::write_u16_le,
};

fn surface() -> SurfaceDescriptor {
    SurfaceDescriptor::new(3, 2, SampleLayout::I2, ColorDescription::SRGB).unwrap()
}

#[test]
fn common_metadata_preserves_distinct_sample_access_contracts() {
    let palette = [17; 16];
    let raw = RawImageAsset::new(surface(), &[&[0x18, 0x1c]])
        .with_color_table(&palette)
        .view()
        .unwrap();
    let source = ImageRef::from(raw);
    assert_eq!(source.raw(), Some(raw));
    assert_eq!(source.encoded(), None);
    let raw_bytes = raw.encode().unwrap();
    let raw = ImageRef::open(&raw_bytes).unwrap();
    assert_eq!(raw.surface(), surface());
    assert_eq!(raw.color_table().unwrap().as_bytes(), palette);
    assert_eq!(raw.raw().unwrap().plane(0).unwrap().bytes(), &[0x18, 0x1c]);
    let access = raw.raw().unwrap().access_capabilities();
    assert!(access.supports_whole());
    assert!(access.supports_rows());
    assert!(access.supports_region());
    assert!(access.supports_direct_borrow());
    assert!(!access.supports_direct_upload());

    let encoded_bytes = EncodedImageAsset::new(surface(), Rle::new().record(), &[1, 0x18, 0x1c])
        .with_color_table(&palette)
        .encode()
        .unwrap();
    let encoded = ImageRef::open(&encoded_bytes).unwrap();
    assert_eq!(encoded.surface(), raw.surface());
    assert_eq!(encoded.color_table(), raw.color_table());
    assert_eq!(encoded.raw(), None);
    let view = encoded.encoded().unwrap();
    assert_eq!(ImageRef::from(view), encoded);
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    let access = groups.access_capabilities().unwrap();
    assert!(access.supports_whole());
    assert!(access.supports_region());
    assert!(!access.supports_rows());
    assert!(!access.supports_direct_borrow());
    assert!(!access.supports_direct_upload());
    groups.validate_unit(0, 0).unwrap();
    let plan = groups
        .get(0)
        .unwrap()
        .get(0)
        .unwrap()
        .decode_plan(SurfaceRequirements::new())
        .unwrap();
    let mut output = [0; 2];
    assert_eq!(
        plan.decode_into(&mut output)
            .unwrap()
            .plane(0)
            .unwrap()
            .bytes(),
        &[0x18, 0x1c]
    );
}

#[test]
fn data_integrity_is_eager_for_raw_and_explicit_for_encoded() {
    let palette = [17; 16];
    let mut raw = RawImageAsset::new(surface(), &[&[0x18, 0x1c]])
        .with_color_table(&palette)
        .encode()
        .unwrap();
    let mut encoded = EncodedImageAsset::new(surface(), Rle::new().record(), &[1, 0x18, 0x1c])
        .with_color_table(&palette)
        .encode()
        .unwrap();
    let raw_offset = data_offset(&raw);
    let encoded_offset = data_offset(&encoded);
    raw[raw_offset] ^= 1;
    encoded[encoded_offset] ^= 1;
    assert!(matches!(
        ImageRef::open(&raw),
        Err(ImageReadError::Raw(RawImageViewError::Media(_)))
    ));
    let view = ImageRef::open(&encoded).unwrap().encoded().unwrap();
    assert!(view.validate_data().is_err());
    for bytes in [&mut raw, &mut encoded] {
        bytes[4] ^= 1;
        assert!(matches!(
            ImageRef::open(bytes),
            Err(ImageReadError::Media(_))
        ));
    }
}

#[test]
fn section_presence_selects_one_parser_without_fallback() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let unknown = CodingRecord::new(CodingId::new(511), 9, &[11]);
    let mut bytes = EncodedImageAsset::new(surface, unknown, &[42])
        .encode()
        .unwrap();
    let view = ImageRef::open(&bytes).unwrap().encoded().unwrap();
    assert_eq!(view.codings().get(0), Some(unknown));
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert_eq!(
        groups.access_capabilities(),
        Err(EncodedImageError::Coding {
            group: 0,
            error: UnitDecodeError::UnsupportedCoding(unknown.id()),
        })
    );
    assert_eq!(
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new()),
        Err(UnitDecodeError::UnsupportedCoding(unknown.id()))
    );
    // A present but optional CODINGS section is not a RAW fallback signal.
    write_u16_le(&mut bytes, 22, 0);
    refresh_crc(&mut bytes);
    assert_eq!(
        ImageRef::open(&bytes),
        Err(ImageReadError::Encoded(
            EncodedImageError::SectionMustBeRequired(MediaSectionKind::CODINGS)
        ))
    );
    // Group records without CODINGS cannot select a different wire generation.
    write_u16_le(&mut bytes, 20, MediaSectionKind::UNIT_GROUPS.raw());
    write_u16_le(&mut bytes, 22, 1);
    refresh_crc(&mut bytes);
    assert_eq!(
        ImageRef::open(&bytes),
        Err(ImageReadError::Raw(RawImageViewError::UnexpectedSection(
            MediaSectionKind::UNIT_GROUPS
        )))
    );
}

#[test]
fn file_position_survives_dispatch_for_both_storage_forms() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let layout = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
        .with_alignment(crate::ByteAlignment::new(64).unwrap())
        .build()
        .unwrap();
    let raw = RawImageAsset::new(surface, &[&[42; 4]])
        .with_memory_layouts(&[layout])
        .encode()
        .unwrap();
    assert!(ImageRef::open_at(&raw, 0).unwrap().raw().is_some());
    assert!(matches!(
        ImageRef::open_at(&raw, 1),
        Err(ImageReadError::Raw(
            RawImageViewError::PlaneFileAddressUnaligned { .. }
        ))
    ));
    let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42])
        .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
        .encode()
        .unwrap();
    let mut slots = [None];
    let view = ImageRef::open_at(&bytes, 0).unwrap().encoded().unwrap();
    assert!(
        view.groups_into(&mut slots, &mut CoverageBudget::new(100))
            .is_ok()
    );
    let view = ImageRef::open_at(&bytes, 1).unwrap().encoded().unwrap();
    assert!(matches!(
        view.groups_into(&mut slots, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::FileAddressUnaligned { .. })
    ));
}
