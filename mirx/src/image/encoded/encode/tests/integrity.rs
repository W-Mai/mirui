use super::*;
use crate::{
    PayloadLimits,
    image::CoverageError,
    media::{DataIntegrity, IntegrityError, MediaPayloadError},
};

#[test]
fn indexed_authoring_has_exact_local_coverage_and_no_whole_data_trailer() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..4).unwrap().with_tiles(2, 1)];
    let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &[0x81, 42, 0x81, 43]);
    let whole = asset.encode().unwrap();
    assert_eq!(asset.integrity(), DataIntegrity::Whole);
    let indexed = asset.with_integrity(DataIntegrity::Indexed(&[2, 4]));
    let mut bytes = indexed.encode().unwrap();
    assert_eq!(bytes.len(), whole.len() + 12 + 24 - 4);
    assert_eq!(indexed.encoded_len(), Ok(bytes.len()));
    assert_eq!(indexed.matches_payload(&bytes), Ok(true));
    let image = EncodedImageView::open(&bytes).unwrap();
    image.preflight(&PayloadLimits::EMBEDDED).unwrap();
    indexed.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let data = image
        .media()
        .section(MediaSectionKind::DATA)
        .unwrap()
        .descriptor()
        .offset();
    let table = image.media().integrity().unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(0).unwrap().range(), data..data + 2);
    assert_eq!(table.get(1).unwrap().range(), data + 2..data + 4);
    assert_eq!(data as usize + 4, bytes.len());
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert_eq!(groups.validate_unit(0, 0), Ok(2));
    assert_eq!(groups.validate_unit(0, 1), Ok(2));
    bytes[data as usize + 3] ^= 1;
    let image = EncodedImageView::open(&bytes).unwrap();
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert_eq!(groups.validate_unit(0, 0), Ok(2));
    assert!(matches!(
        groups.validate_unit(0, 1),
        Err(EncodedImageError::Media(
            MediaPayloadError::RangeCrcMismatch { .. }
        ))
    ));
    assert!(image.preflight(&PayloadLimits::EMBEDDED).is_err());
    assert_eq!(indexed.matches_payload(&bytes), Ok(false));
    assert_eq!(
        indexed
            .with_integrity(DataIntegrity::Whole)
            .encode()
            .unwrap(),
        whole
    );
}

#[test]
fn partitioned_padding_and_work_bounds_match_wire_admission() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..66)
        .unwrap()
        .with_tiles(2, 1)
        .with_input_alignment(crate::ByteAlignment::new(64).unwrap())];
    let mut data = [0xa5; 66];
    data[..2].copy_from_slice(&[0x81, 42]);
    data[64..].copy_from_slice(&[0x81, 43]);
    for ends in [&[64, 66][..], &[2, 64, 66][..]] {
        let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &data)
            .with_integrity(DataIntegrity::Indexed(ends));
        let payload = asset.encode().unwrap();
        let image = EncodedImageView::open_at(&payload, 0).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1024))
            .unwrap();
        assert_eq!(groups.validate_unit(0, 0), Ok(ends[0]));
        assert_eq!(groups.validate_unit(0, 1), Ok(2));
        let minimum = (0..8192)
            .find(|work| {
                image
                    .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(*work))
                    .is_ok()
            })
            .unwrap();
        let combined = minimum + payload.len() as u64;
        assert!(
            asset
                .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(combined - 1))
                .is_err()
        );
        asset
            .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(combined))
            .unwrap();
    }
}

#[test]
fn invalid_partition_shapes_and_early_size_limits_cannot_touch_output() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42]);
    let mut output = [0xad; 256];
    for ends in [&[][..], &[0, 2], &[1], &[3], &[2, 2], &[1, 0, 2]] {
        let invalid = asset.with_integrity(DataIntegrity::Indexed(ends));
        assert!(matches!(
            invalid.encode_into(&mut output),
            Err(ImageEncodeError::Integrity(_))
        ));
        assert_eq!(output, [0xad; 256]);
    }
    let many = [0; 128];
    assert_eq!(
        asset
            .with_integrity(DataIntegrity::Indexed(&many))
            .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(1535)),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coverage(
            CoverageError::BudgetExceeded
        )))
    );
    let empty =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(empty, Rle::new().record(), &[])
        .with_integrity(DataIntegrity::Indexed(&[]));
    let bytes = asset.encode().unwrap();
    assert_eq!(bytes.len(), 100);
    let image = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(image.media().integrity().unwrap().len(), 0);
    image.preflight(&PayloadLimits::EMBEDDED).unwrap();
    asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        asset
            .with_integrity(DataIntegrity::Indexed(&[0]))
            .encoded_len(),
        Err(ImageEncodeError::Integrity(
            IntegrityError::RangesOverlapOrReversed { index: 0 }
        ))
    );
}
