use super::*;
use crate::{
    PayloadLimits,
    image::{CoverageError, GroupPlanes, ReferenceMode},
    media::UnitIndexEncoding,
};

#[test]
fn planar_groups_share_profiles_and_preserve_alignment_without_unit_tables() {
    let surface = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let codings = [Rle::new().record()];
    let records = [
        UnitGroupRecord::new(0, 0..2)
            .unwrap()
            .with_planes(GroupPlanes::Plane(0)),
        UnitGroupRecord::new(0, 64..66)
            .unwrap()
            .with_planes(GroupPlanes::Plane(1))
            .with_input_alignment(64),
    ];
    let mut data = [0xa5; 66];
    data[..2].copy_from_slice(&[0x88, 16]);
    data[64..].copy_from_slice(&[0x87, 128]);
    let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &data);
    assert_eq!(asset.codings(), &codings);
    assert_eq!(asset.groups(), Some(records.as_slice()));
    assert_eq!(asset.index(), &[]);
    assert_eq!(asset.input_alignment(), Ok(64));
    asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let payload = asset.encode().unwrap();
    let view = EncodedImageView::open_at(&payload, 0).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(view.codings().len(), 1);
    assert_eq!(view.group_count(), 2);
    assert_eq!(view.input_alignment(), Ok(64));
    assert!(view.media().section(MediaSectionKind::UNIT_INDEX).is_none());
    assert_eq!(
        view.media()
            .section(MediaSectionKind::DATA)
            .unwrap()
            .bytes(),
        data
    );
    let mut slots = [None; 2];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(1024))
        .unwrap();
    for (index, value, expected) in [(0, 16, 9), (1, 128, 8)] {
        let unit = groups.get(index).unwrap().get(0).unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(64))
            .unwrap();
        let mut output = [0xad; 256];
        let decoded = plan.decode_into(&mut output).unwrap();
        let plane = decoded.plane(index as u8).unwrap();
        assert_eq!(
            plane.rows().unwrap().map(|r| r.len()).sum::<usize>(),
            expected
        );
        assert!(plane.rows().unwrap().flatten().all(|b| *b == value));
    }
    assert_eq!(asset.matches_payload(&payload), Ok(true));
    let mut output = [0xad; 512];
    let len = asset.encode_into(&mut output).unwrap();
    assert_eq!(&output[..len], payload);
    assert!(output[len..].iter().all(|b| *b == 0xad));
    for index in 0..len {
        output[index] ^= 1;
        assert_eq!(asset.matches_payload(&output[..len]), Ok(false));
        output[index] ^= 1;
    }
}

#[test]
fn variable_indexes_preserve_exact_lengths_padding_and_canonical_work() {
    let surface = SurfaceDescriptor::new(5, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    for encoding in [
        UnitIndexEncoding::Offsets,
        UnitIndexEncoding::Lengths16,
        UnitIndexEncoding::Lengths32,
    ] {
        let alignment = if encoding == UnitIndexEncoding::Offsets {
            1
        } else {
            64
        };
        let lengths = [3, 2, 2];
        let mut index_bytes = [0; 32];
        let index_len = encoding
            .encode_into(&lengths, alignment, &mut index_bytes)
            .unwrap();
        let mut data = [0xa5; 130];
        let mut end = 0usize;
        for stream in [&[1, 1, 2][..], &[0x81, 3][..], &[0, 4][..]] {
            let start = UnitIndex::aligned(end as u32, alignment).unwrap() as usize;
            end = start + stream.len();
            data[start..end].copy_from_slice(stream);
        }
        let records = [UnitGroupRecord::new(0, 0..end as u32)
            .unwrap()
            .with_tiles(2, 1)
            .with_input_alignment(alignment)
            .with_index_encoding(encoding)];
        let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &data[..end])
            .with_index(&index_bytes[..index_len]);
        let payload = asset.encode().unwrap();
        let image = EncodedImageView::open(&payload).unwrap();
        assert_eq!(
            image
                .media()
                .section(MediaSectionKind::UNIT_INDEX)
                .unwrap()
                .bytes(),
            asset.index()
        );
        let minimum = (0..8192)
            .find(|work| {
                image
                    .preflight(&PayloadLimits::EMBEDDED.with_max_image_work(*work))
                    .is_ok()
            })
            .unwrap();
        let combined = minimum + payload.len() as u64;
        assert!(
            asset
                .preflight(&PayloadLimits::EMBEDDED.with_max_image_work(combined - 1))
                .is_err()
        );
        asset
            .preflight(&PayloadLimits::EMBEDDED.with_max_image_work(combined))
            .unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1024))
            .unwrap();
        for (ordinal, expected) in [&[1, 2][..], &[3, 3][..], &[4][..]].into_iter().enumerate() {
            let unit = groups.get(0).unwrap().get(ordinal).unwrap();
            assert_eq!(unit.data().len(), lengths[ordinal] as usize);
            let plan = unit.decode_plan(SurfaceRequirements::new()).unwrap();
            let mut output = [0; 2];
            assert_eq!(
                plan.decode_into(&mut output)
                    .unwrap()
                    .plane(0)
                    .unwrap()
                    .bytes(),
                expected
            );
        }
    }
}

#[test]
fn structural_errors_preserve_output_and_coverage_requires_bounded_admission() {
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..2).unwrap()];
    let valid = EncodedImageAsset::from_groups(surface, &codings, &records, &[0x81, 42]);
    let malformed = [records[0].with_input_alignment(3)];
    let reference = [records[0].with_reference(ReferenceMode::Previous)];
    let missing_coding = [UnitGroupRecord::new(1, 0..2).unwrap()];
    let unreferenced = [UnitGroupRecord::new(0, 0..1).unwrap()];
    let mut output = [0xad; 512];
    for invalid in [
        EncodedImageAsset::from_groups(surface, &[], &records, &[0x81, 42]),
        EncodedImageAsset::from_groups(surface, &codings, &[], &[0x81, 42]),
        EncodedImageAsset::from_groups(surface, &codings, &malformed, &[0x81, 42]),
        EncodedImageAsset::from_groups(surface, &codings, &reference, &[0x81, 42]),
        EncodedImageAsset::from_groups(surface, &codings, &missing_coding, &[0x81, 42]),
        EncodedImageAsset::from_groups(surface, &codings, &unreferenced, &[0x81, 42]),
        valid.with_input_alignment(64),
        valid.with_index(&[0]),
        EncodedImageAsset::new(surface, codings[0], &[0x81, 42]).with_index(&[0]),
    ] {
        assert!(invalid.encode_into(&mut output).is_err());
        assert_eq!(output, [0xad; 512]);
    }
    assert_eq!(
        valid.with_input_alignment(64).encoded_len(),
        Err(ImageEncodeError::ConflictingAlignment)
    );
    let overlapping = [records[0], UnitGroupRecord::new(0, 2..4).unwrap()];
    let asset =
        EncodedImageAsset::from_groups(surface, &codings, &overlapping, &[0x81, 42, 0x81, 43]);
    let bytes = asset.encode().unwrap();
    assert!(matches!(
        asset.preflight(&PayloadLimits::EMBEDDED),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coverage(_)))
    ));
    assert!(
        EncodedImageView::open(&bytes)
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED)
            .is_err()
    );
    let many_codings = [codings[0]; 32];
    let asset = EncodedImageAsset::from_groups(surface, &many_codings, &malformed, &[0x81, 42]);
    // Resource gates must run before scanning even malformed native records.
    assert!(matches!(
        asset.preflight(&PayloadLimits::EMBEDDED.with_max_image_groups(0)),
        Err(ImageEncodeError::Preflight(
            EncodedImageError::TooManyGroups { .. }
        ))
    ));
    assert_eq!(
        asset.preflight(&PayloadLimits::EMBEDDED.with_max_image_work(255)),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coverage(
            CoverageError::BudgetExceeded
        )))
    );
}
