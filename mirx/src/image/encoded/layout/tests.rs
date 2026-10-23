use super::*;
use crate::{
    coding::Rle,
    image::{
        ColorDescription, EncodedImageAsset, EncodedImageView, SampleLayout, SurfaceRequirements,
        UnitGroup,
    },
    media::{MediaSectionKind, UnitSelection, UnitSelectionEncoding},
};
use alloc::vec::Vec;

#[test]
fn separate_metadata_windows_bind_only_selected_inputs_to_the_existing_decoder() {
    let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let coding = Rle::new();
    let mut left = [0; 8];
    let mut right = [0; 8];
    UnitSelectionEncoding::List
        .encode_into(4, &[0, 2], &mut left)
        .unwrap();
    UnitSelectionEncoding::List
        .encode_into(4, &[1, 3], &mut right)
        .unwrap();
    let a = [0x81, 11, 0x81, 33];
    let b = [0x81, 22, 0x81, 44];
    let groups = [
        UnitGroup::builder(surface, coding.record(), &a)
            .with_tiles(2, 1)
            .with_selection(UnitSelection::list(4, &left).unwrap())
            .build()
            .unwrap(),
        UnitGroup::builder(surface, coding.record(), &b)
            .with_tiles(2, 1)
            .with_selection(UnitSelection::list(4, &right).unwrap())
            .build()
            .unwrap(),
    ];
    let bytes = EncodedImageAsset::from_groups(surface, &groups)
        .encode()
        .unwrap();
    let image = EncodedImageView::open(&bytes).unwrap();
    let section = |kind| {
        image
            .media()
            .sections_of_kind(kind)
            .next()
            .map(|s| s.bytes().to_vec())
    };
    let codings = section(MediaSectionKind::CODINGS).unwrap();
    let records = section(MediaSectionKind::UNIT_GROUPS);
    let indexes = section(MediaSectionKind::UNIT_INDEX);
    let data = section(MediaSectionKind::DATA).unwrap();
    // Only detached metadata and the test's simulated source remain alive.
    drop(bytes);
    let layout = EncodedImageLayout::new(
        surface,
        &codings,
        records.as_deref(),
        indexes.as_deref(),
        data.len() as u32,
        None,
    )
    .unwrap();
    layout
        .validate_groups(&mut CoverageBudget::new(1000))
        .unwrap();
    assert_eq!(layout.group_count(), 2);
    assert_eq!(layout.surface(), surface);
    for (index, expected) in [[11, 33], [22, 44]].into_iter().enumerate() {
        let group = layout.group(index).unwrap();
        assert_eq!(group.len(), 2);
        let query = surface.region(2, 0, 2, 2).unwrap();
        let selected: Vec<_> = group.units_in(query).unwrap().collect();
        assert_eq!(
            selected,
            if index == 0 {
                Vec::new()
            } else {
                alloc::vec![0, 1]
            }
        );
        for (ordinal, value) in expected.into_iter().enumerate() {
            let range = group.unit_range(ordinal).unwrap();
            let input = &data[range.start as usize..range.end as usize];
            let unit = group.unit(ordinal, input).unwrap();
            assert_eq!(Some(unit.region()), group.unit_region(ordinal));
            let mut output = [0; 2];
            unit.decode_plan(SurfaceRequirements::new())
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
            assert_eq!(output, [value; 2]);
            assert!(group.unit(ordinal, &input[..1]).is_err());
            assert!(group.unit(ordinal, &[0; 3]).is_err());
        }
        assert_eq!(group.unit_range(2), None);
        assert_eq!(group.unit_region(2), None);
        assert!(matches!(
            group.unit(2, &[]),
            Err(EncodedImageError::UnitOutOfBounds { .. })
        ));
        assert!(group.units_in(Region::new(4, 0, 1, 1).unwrap()).is_err());
    }
    assert!(layout.group(2).is_err());
}

#[test]
fn layout_and_resident_validation_share_alignment_coverage_and_work_limits() {
    use super::super::{
        groups::{CodingRecords, GroupRecords, GroupSource},
        tests::payload,
    };
    use crate::image::UnitGroupRecord;
    use crate::image::{GroupPlanes, ReferenceMode};
    let surface = SurfaceDescriptor::new(
        3,
        2,
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
            .with_input_alignment(ByteAlignment::new(64).unwrap()),
    ];
    let mut data = [0; 66];
    data[..2].copy_from_slice(&[0x85, 42]);
    data[64..].copy_from_slice(&[0x83, 128]);
    for invalid in 0..4 {
        let mut records = records;
        match invalid {
            1 => records[1] = records[1].with_reference(ReferenceMode::Previous),
            2 => records[1] = records[1].with_planes(GroupPlanes::Plane(0)),
            3 => records[1] = UnitGroupRecord::new(0, 63..65).unwrap(),
            _ => {}
        }
        let bytes = payload(surface, &codings, Some(&records), None, &data, None, None);
        let view = EncodedImageView::open(&bytes).unwrap();
        let layout = view.layout();
        let source = GroupSource {
            surface,
            codings: CodingRecords::Native(&codings),
            records: GroupRecords::Native(&records),
            data: &data,
            indexes: &[],
            file_offset: None,
            data_offset: 0,
        };
        for limit in [0, 32, 512, 4096] {
            let mut a = CoverageBudget::new(limit);
            let mut b = CoverageBudget::new(limit);
            assert_eq!(
                layout.validate_groups(&mut a),
                source.metadata().unwrap().validate_groups(&mut b)
            );
            assert_eq!(a.remaining(), b.remaining());
        }
        if invalid == 0 {
            layout
                .validate_groups(&mut CoverageBudget::new(4096))
                .unwrap();
            let group = layout.group(1).unwrap();
            assert_eq!(group.unit_range(0), Some(64..66));
            assert_eq!(group.input_alignment().get(), 64);
            assert_eq!(group.planes(), GroupPlanes::Plane(1));
            let mut output = [0; 4];
            group
                .unit(0, &data[64..])
                .unwrap()
                .decode_plan(SurfaceRequirements::new())
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
            assert_eq!(output, [128; 4]);
        }
    }
}
