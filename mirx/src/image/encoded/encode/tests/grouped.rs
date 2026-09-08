use alloc::vec;

use super::*;
use crate::{
    PayloadLimits,
    image::{CoverageError, GroupPlanes, GroupSelection, ReferenceMode},
    media::{UnitIndex, UnitIndexEncoding, UnitSelection},
};

fn encoded_group(payload: &[u8], index: usize) -> UnitGroupRecord {
    let image = EncodedImageView::open(payload).unwrap();
    let records = image
        .media()
        .section(MediaSectionKind::UNIT_GROUPS)
        .unwrap()
        .bytes();
    UnitGroupRecord::open(&records[index * UNIT_GROUP_RECORD_LEN..]).unwrap()
}

#[test]
fn semantic_groups_deduplicate_codings_and_choose_selection_storage() {
    let surface = SurfaceDescriptor::new(512, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let sparse_cells = [1, 510];
    let dense_cells: [u32; 510] = core::array::from_fn(|index| match index {
        0 => 0,
        509 => 511,
        _ => index as u32 + 1,
    });
    let sparse_data = [0x80, 7, 0x80, 9];
    let mut dense_data = [0; 1020];
    for stream in dense_data.chunks_exact_mut(2) {
        stream.copy_from_slice(&[0x80, 5]);
    }
    let coding = Rle::new().record();
    let groups = [
        UnitGroup::builder(surface, coding, &sparse_data)
            .with_tiles(1, 1)
            .with_selection(UnitSelection::cells(512, &sparse_cells).unwrap())
            .build()
            .unwrap(),
        UnitGroup::builder(surface, coding, &dense_data)
            .with_tiles(1, 1)
            .with_selection(UnitSelection::cells(512, &dense_cells).unwrap())
            .build()
            .unwrap(),
    ];
    let asset = EncodedImageAsset::from_groups(surface, &groups);
    asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let payload = asset.encode().unwrap();
    let image = EncodedImageView::open(&payload).unwrap();
    assert_eq!(image.codings().len(), 1);
    assert_eq!(
        encoded_group(&payload, 0).selection(),
        GroupSelection::List(2)
    );
    assert_eq!(encoded_group(&payload, 0).index_offset(), 0);
    assert_eq!(
        encoded_group(&payload, 1).selection(),
        GroupSelection::Bitmap
    );
    assert_eq!(encoded_group(&payload, 1).index_offset(), 8);
    assert_eq!(
        image
            .media()
            .section(MediaSectionKind::UNIT_INDEX)
            .unwrap()
            .bytes()
            .len(),
        80
    );
}

#[test]
fn semantic_coding_iteration_preserves_first_occurrence_order() {
    let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let data = [[1], [2], [3], [4], [5]];
    let a = CodingRecord::new(CodingId::new(300), 1, &[]);
    let b = CodingRecord::new(CodingId::new(301), 1, &[]);
    let c = CodingRecord::new(CodingId::new(302), 1, &[]);
    let codings = [a, b, a, c, b];
    let groups: [UnitGroup<'_>; 5] = core::array::from_fn(|index| {
        UnitGroup::builder(surface, codings[index], &data[index])
            .build()
            .unwrap()
    });
    let asset = EncodedImageAsset::from_groups(surface, &groups);
    assert_eq!(asset.codings().collect::<Vec<_>>(), [a, b, c]);
    assert_eq!(asset.codings().rev().collect::<Vec<_>>(), [c, b, a]);
    let mut codings = asset.codings();
    assert_eq!(codings.next(), Some(a));
    assert_eq!(codings.next_back(), Some(c));
    assert_eq!(codings.len(), 1);
    assert_eq!(codings.next(), Some(b));
    assert_eq!(codings.next(), None);
}

#[test]
fn semantic_groups_choose_canonical_range_storage() {
    let coding = Rle::new().record();

    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let fixed_data = [0; 4];
    let fixed = [UnitGroup::builder(surface, coding, &fixed_data)
        .with_tiles(1, 1)
        .build()
        .unwrap()];
    let payload = EncodedImageAsset::from_groups(surface, &fixed)
        .encode()
        .unwrap();
    assert_eq!(encoded_group(&payload, 0).index_encoding(), None);

    let alignment = crate::ByteAlignment::new(64).unwrap();
    let lengths16_data = [0; 67];
    let lengths16_ranges = [0..2, 64..67];
    let lengths16 = [UnitGroup::builder(surface, coding, &lengths16_data)
        .with_tiles(1, 1)
        .with_input_alignment(alignment)
        .with_index(UnitIndex::ranges(&lengths16_ranges, alignment).unwrap())
        .build()
        .unwrap()];
    let payload = EncodedImageAsset::from_groups(surface, &lengths16)
        .encode()
        .unwrap();
    assert_eq!(
        encoded_group(&payload, 0).index_encoding(),
        Some(UnitIndexEncoding::Lengths16)
    );

    let alignment = crate::ByteAlignment::new(2).unwrap();
    let lengths32_data = vec![0; 65_539];
    let lengths32_ranges = [0..65_537, 65_538..65_539];
    let lengths32 = [UnitGroup::builder(surface, coding, &lengths32_data)
        .with_tiles(1, 1)
        .with_input_alignment(alignment)
        .with_index(UnitIndex::ranges(&lengths32_ranges, alignment).unwrap())
        .build()
        .unwrap()];
    let payload = EncodedImageAsset::from_groups(surface, &lengths32)
        .encode()
        .unwrap();
    assert_eq!(
        encoded_group(&payload, 0).index_encoding(),
        Some(UnitIndexEncoding::Lengths32)
    );

    let surface = SurfaceDescriptor::new(65, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut offset_ranges = Vec::with_capacity(65);
    offset_ranges.push(0..65_536);
    for start in 65_536..65_600 {
        offset_ranges.push(start..start + 1);
    }
    let offset_data = vec![0; 65_600];
    let offsets = [UnitGroup::builder(surface, coding, &offset_data)
        .with_tiles(1, 1)
        .with_index(UnitIndex::ranges(&offset_ranges, crate::ByteAlignment::ONE).unwrap())
        .build()
        .unwrap()];
    let payload = EncodedImageAsset::from_groups(surface, &offsets)
        .encode()
        .unwrap();
    assert_eq!(
        encoded_group(&payload, 0).index_encoding(),
        Some(UnitIndexEncoding::Offsets)
    );
}

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
            .with_input_alignment(crate::ByteAlignment::new(64).unwrap()),
    ];
    let mut data = [0xa5; 66];
    data[..2].copy_from_slice(&[0x88, 16]);
    data[64..].copy_from_slice(&[0x87, 128]);
    let asset = EncodedImageAsset::from_records(surface, &codings, &records, &data);
    assert_eq!(asset.codings().collect::<Vec<_>>(), &codings);
    assert_eq!(asset.group_records(), Some(records.as_slice()));
    assert!(asset.unit_index().is_empty());
    assert_eq!(
        asset.input_alignment().map(crate::ByteAlignment::get),
        Ok(64)
    );
    asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let payload = asset.encode().unwrap();
    let view = EncodedImageView::open_at(&payload, 0).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(view.codings().len(), 1);
    assert_eq!(view.group_count(), 2);
    assert_eq!(
        view.input_alignment().map(crate::ByteAlignment::get),
        Ok(64)
    );
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
        let alignment = crate::ByteAlignment::new(if encoding == UnitIndexEncoding::Offsets {
            1
        } else {
            64
        })
        .unwrap();
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
        let asset = EncodedImageAsset::from_records(surface, &codings, &records, &data[..end])
            .with_unit_index(&index_bytes[..index_len]);
        let payload = asset.encode().unwrap();
        let image = EncodedImageView::open(&payload).unwrap();
        assert_eq!(
            image
                .media()
                .section(MediaSectionKind::UNIT_INDEX)
                .unwrap()
                .bytes(),
            asset.unit_index()
        );
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
    let valid = EncodedImageAsset::from_records(surface, &codings, &records, &[0x81, 42]);
    let reference = [records[0].with_reference(ReferenceMode::Previous)];
    let missing_coding = [UnitGroupRecord::new(1, 0..2).unwrap()];
    let unreferenced = [UnitGroupRecord::new(0, 0..1).unwrap()];
    let mut output = [0xad; 512];
    for invalid in [
        EncodedImageAsset::from_records(surface, &[], &records, &[0x81, 42]),
        EncodedImageAsset::from_records(surface, &codings, &[], &[0x81, 42]),
        EncodedImageAsset::from_records(surface, &codings, &reference, &[0x81, 42]),
        EncodedImageAsset::from_records(surface, &codings, &missing_coding, &[0x81, 42]),
        EncodedImageAsset::from_records(surface, &codings, &unreferenced, &[0x81, 42]),
        valid.with_input_alignment(crate::ByteAlignment::new(64).unwrap()),
        valid.with_unit_index(&[0]),
        EncodedImageAsset::new(surface, codings[0], &[0x81, 42]).with_unit_index(&[0]),
    ] {
        assert!(invalid.encode_into(&mut output).is_err());
        assert_eq!(output, [0xad; 512]);
    }
    assert!(crate::ByteAlignment::new(3).is_err());
    assert_eq!(
        valid
            .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
            .encoded_len(),
        Err(ImageEncodeError::ConflictingAlignment)
    );
    let overlapping = [records[0], UnitGroupRecord::new(0, 2..4).unwrap()];
    let asset =
        EncodedImageAsset::from_records(surface, &codings, &overlapping, &[0x81, 42, 0x81, 43]);
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
    let asset = EncodedImageAsset::from_records(surface, &many_codings, &reference, &[0x81, 42]);
    // Resource gates must run before scanning even malformed native records.
    assert!(matches!(
        asset.preflight(&PayloadLimits::EMBEDDED.with_max_raster_groups(0)),
        Err(ImageEncodeError::Preflight(
            EncodedImageError::TooManyGroups { .. }
        ))
    ));
    assert_eq!(
        asset.preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(255)),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coverage(
            CoverageError::BudgetExceeded
        )))
    );

    let empty = EncodedImageAsset::from_groups(surface, &[]);
    assert_eq!(
        empty.encoded_len(),
        Err(ImageEncodeError::Preflight(
            EncodedImageError::InvalidGroupTableLength(0)
        ))
    );
    let stream = [0x81, 42];
    let independent = UnitGroup::builder(surface, codings[0], &stream)
        .build()
        .unwrap();
    let previous = UnitGroup::builder(surface, codings[0], &stream)
        .with_reference(ReferenceMode::Previous)
        .build()
        .unwrap();
    let groups = [independent, previous];
    assert_eq!(
        EncodedImageAsset::from_groups(surface, &groups).encoded_len(),
        Err(ImageEncodeError::Preflight(
            EncodedImageError::ReferenceInStaticImage(1)
        ))
    );
}
