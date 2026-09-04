use super::super::tests::payload;
use super::*;

use crate::{
    coding::{Lz4, Pixel, Rle},
    image::{
        ColorDescription, CoverageError, EncodedImageAsset, SampleLayout, SurfaceDescriptor,
        UnitGroupRecord,
        test_support::{data_offset, refresh_crc},
    },
    media::{CodingId, CodingRecord},
};

#[test]
fn scalar_syntax_and_integrity_are_both_required() {
    let surface =
        SurfaceDescriptor::new(2, 2, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let samples = [42; 12];
    let pixel = Pixel::new(SampleLayout::RGB888).unwrap();
    let rle = Rle::new();
    let lz4 = Lz4::new();
    for coding in [pixel.record(), rle.record(), lz4.record()] {
        let mut stream = [0; 32];
        let len = match coding.id() {
            CodingId::PIXEL => pixel.encode_into(&samples, &mut stream).unwrap(),
            CodingId::RLE => rle.encode_into(&samples, &mut stream).unwrap(),
            _ => lz4
                .encoder(&mut [0; Lz4::TABLE_LEN])
                .unwrap()
                .encode_into(&samples, &mut stream)
                .unwrap(),
        };
        let bytes = EncodedImageAsset::new(surface, coding, &stream[..len])
            .encode()
            .unwrap();
        EncodedImageView::open(&bytes)
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED)
            .unwrap();
        let malformed = EncodedImageAsset::new(surface, coding, &[0xff])
            .encode()
            .unwrap();
        assert!(matches!(
            EncodedImageView::open(&malformed)
                .unwrap()
                .preflight(&PayloadLimits::EMBEDDED),
            Err(EncodedImageError::Unit {
                group: 0,
                ordinal: 0,
                ..
            })
        ));
    }
    let mut bytes = EncodedImageAsset::new(surface, rle.record(), &[0x8b, 42])
        .encode()
        .unwrap();
    let data = data_offset(&bytes);
    bytes[data + 1] ^= 1;
    assert!(matches!(
        EncodedImageView::open(&bytes)
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED),
        Err(EncodedImageError::Media(_))
    ));
    refresh_crc(&mut bytes);
    EncodedImageView::open(&bytes)
        .unwrap()
        .preflight(&PayloadLimits::EMBEDDED)
        .unwrap();
}

#[test]
fn limits_bound_independent_units_instead_of_allocating_the_full_surface() {
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let record = UnitGroupRecord::new(0, 0..8).unwrap().with_tiles(2, 1);
    let bytes = payload(
        surface,
        &[Rle::new().record()],
        Some(&[record]),
        None,
        &[0x81, 42, 0x81, 43, 0x81, 44, 0x81, 45],
        None,
        None,
    );
    let view = EncodedImageView::open(&bytes).unwrap();
    let limits = PayloadLimits::EMBEDDED
        .with_max_decoded_bytes(2)
        .with_max_image_groups(1)
        .with_max_image_units(4);
    assert_eq!(view.preflight(&limits), Ok(()));
    assert_eq!(
        view.preflight(&limits.with_max_decoded_bytes(1)),
        Err(EncodedImageError::DecodedUnitTooLarge {
            group: 0,
            ordinal: 0,
            limit: 1,
            actual: 2
        })
    );
    assert_eq!(
        view.preflight(&limits.with_max_image_units(3)),
        Err(EncodedImageError::TooManyUnits {
            limit: 3,
            actual: 4
        })
    );
    assert_eq!(
        view.preflight(&limits.with_max_image_groups(0)),
        Err(EncodedImageError::TooManyGroups {
            limit: 0,
            actual: 1
        })
    );
    let minimum = (0..1024)
        .find(|work| view.preflight(&limits.with_max_image_work(*work)).is_ok())
        .unwrap();
    assert!(minimum > 8 + 8 + 8);
    assert_eq!(
        view.preflight(&limits.with_max_image_work(minimum - 1)),
        Err(EncodedImageError::Coverage(CoverageError::BudgetExceeded))
    );
    let indexed = payload(
        surface,
        &[Rle::new().record()],
        Some(&[record]),
        None,
        &[0x81, 42, 0x81, 43, 0x81, 44, 0x81, 45],
        None,
        Some(&[2, 2, 2, 2]),
    );
    EncodedImageView::open(&indexed)
        .unwrap()
        .preflight(&limits)
        .unwrap();
}

#[test]
fn empty_surfaces_still_require_an_understood_active_profile() {
    let surface =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let limits = PayloadLimits::EMBEDDED
        .with_max_image_units(0)
        .with_max_decoded_bytes(0);
    for coding in [Rle::new().record(), Lz4::new().record()] {
        let bytes = EncodedImageAsset::new(surface, coding, &[])
            .encode()
            .unwrap();
        EncodedImageView::open(&bytes)
            .unwrap()
            .preflight(&limits)
            .unwrap();
    }
    for coding in [
        CodingRecord::new(CodingId::new(511), 1, &[]),
        CodingRecord::new(CodingId::LZ4, 2, &[]),
        CodingRecord::new(CodingId::RLE, 1, &[1]),
        Pixel::new(SampleLayout::RGB888).unwrap().record(),
    ] {
        let bytes = EncodedImageAsset::new(surface, coding, &[])
            .encode()
            .unwrap();
        assert!(matches!(
            EncodedImageView::open(&bytes).unwrap().preflight(&limits),
            Err(EncodedImageError::Coding { group: 0, .. })
        ));
    }
    let rgb =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let bytes =
        EncodedImageAsset::new(rgb, Pixel::new(SampleLayout::RGB888).unwrap().record(), &[])
            .encode()
            .unwrap();
    EncodedImageView::open(&bytes)
        .unwrap()
        .preflight(&limits)
        .unwrap();
}

#[test]
fn coded_limits_and_errors_follow_group_and_unit_locations() {
    use crate::image::{GroupPlanes, ReferenceMode};
    let surface = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let records = [
        UnitGroupRecord::new(0, 0..2)
            .unwrap()
            .with_planes(GroupPlanes::Plane(0)),
        UnitGroupRecord::new(1, 2..4)
            .unwrap()
            .with_planes(GroupPlanes::Plane(1)),
    ];
    let bytes = payload(
        surface,
        &[Rle::new().record(), Rle::new().record()],
        Some(&records),
        None,
        &[0x88, 16, 0x87, 128],
        None,
        None,
    );
    let view = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(
        view.preflight(&PayloadLimits::EMBEDDED.with_max_image_groups(1)),
        Err(EncodedImageError::TooManyGroups {
            limit: 1,
            actual: 2
        })
    );
    assert_eq!(
        view.preflight(&PayloadLimits::EMBEDDED.with_max_image_units(1)),
        Err(EncodedImageError::TooManyUnits {
            limit: 1,
            actual: 2
        })
    );
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let malformed = payload(
        surface,
        &[Rle::new().record(), Rle::new().record()],
        Some(&records),
        None,
        &[0x88, 16, 0x86, 128],
        None,
        None,
    );
    assert!(matches!(
        EncodedImageView::open(&malformed)
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED),
        Err(EncodedImageError::Unit {
            group: 1,
            ordinal: 0,
            ..
        })
    ));
    let mut temporal = records;
    temporal[1] = temporal[1].with_reference(ReferenceMode::Previous);
    let bytes = payload(
        surface,
        &[Rle::new().record(), Rle::new().record()],
        Some(&temporal),
        None,
        &[0x88, 16, 0x87, 128],
        None,
        None,
    );
    assert_eq!(
        EncodedImageView::open(&bytes)
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED),
        Err(EncodedImageError::ReferenceInStaticImage(1))
    );
}
