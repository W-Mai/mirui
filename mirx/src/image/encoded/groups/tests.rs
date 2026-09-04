use super::super::{EncodedImageView, preflight::Preflight, tests::payload};
use super::*;
use crate::{
    PayloadLimits,
    coding::Rle,
    image::{ColorDescription, GroupPlanes, SampleLayout},
};

#[test]
fn native_and_wire_groups_share_coverage_indexes_alignment_and_work() {
    let surface = SurfaceDescriptor::new(
        3,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let codings = [Rle::new().record(), Rle::new().record()];
    let records = [
        UnitGroupRecord::new(0, 0..2)
            .unwrap()
            .with_planes(GroupPlanes::Plane(0)),
        UnitGroupRecord::new(1, 64..66)
            .unwrap()
            .with_planes(GroupPlanes::Plane(1))
            .with_input_alignment(64),
    ];
    let mut data = [0; 66];
    data[..2].copy_from_slice(&[0x85, 42]);
    data[64..].copy_from_slice(&[0x83, 128]);
    for mode in 0..5 {
        let mut records = records;
        let mut data = data;
        match mode {
            1 => records[1] = records[1].with_planes(GroupPlanes::Plane(0)),
            2 => records[1] = records[1].with_reference(ReferenceMode::Previous),
            3 => data[64] = 0xff,
            4 => {
                records[1] = records[1]
                    .with_index_offset(1)
                    .with_index_encoding(crate::media::UnitIndexEncoding::Offsets)
            }
            _ => {}
        }
        let native = GroupSource {
            surface,
            codings: CodingRecords::Native(&codings),
            records: GroupRecords::Native(&records),
            data: &data,
            indexes: &[],
            file_offset: None,
            data_offset: 0,
        };
        let bytes = payload(surface, &codings, Some(&records), None, &data, None, None);
        let wire = EncodedImageView::open(&bytes).unwrap().group_source();
        assert_eq!(native.input_alignment(), wire.input_alignment());
        for work in [0, 1, 16, 128, 512, 2048] {
            let mut a = CoverageBudget::new(work);
            let mut b = CoverageBudget::new(work);
            assert_eq!(native.validate_groups(&mut a), wire.validate_groups(&mut b));
            assert_eq!(a.remaining(), b.remaining());
            let limits = PayloadLimits::EMBEDDED.with_max_raster_work(work);
            let mut a = Preflight::new(&limits, 2).unwrap();
            let mut b = Preflight::new(&limits, 2).unwrap();
            assert_eq!(a.groups(native), b.groups(wire));
        }
        if mode == 0 {
            let mut preflight = Preflight::new(&PayloadLimits::EMBEDDED, 2).unwrap();
            preflight.groups(native).unwrap();
            assert_eq!(
                native.record(2),
                Err(EncodedImageError::GroupOutOfBounds(2))
            );
        }
    }
}

#[test]
fn native_records_cannot_bypass_wire_field_validation_or_implicit_rules() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let source = GroupSource {
        surface,
        codings: CodingRecords::Native(&codings),
        records: GroupRecords::Implicit,
        data: &[0x83, 42],
        indexes: &[],
        file_offset: None,
        data_offset: 0,
    };
    source
        .validate_groups(&mut CoverageBudget::new(100))
        .unwrap();
    let invalid = [UnitGroupRecord::new(0, 0..2)
        .unwrap()
        .with_input_alignment(3)];
    let invalid = GroupSource {
        records: GroupRecords::Native(&invalid),
        ..source
    };
    assert!(invalid.input_alignment().is_err());
    assert!(
        invalid
            .validate_groups(&mut CoverageBudget::new(100))
            .is_err()
    );
    let empty = GroupSource {
        records: GroupRecords::Native(&[]),
        ..source
    };
    assert_eq!(
        empty.validate_groups(&mut CoverageBudget::new(100)),
        Err(EncodedImageError::InvalidGroupTableLength(0))
    );
    let ambiguous = GroupSource {
        indexes: &[0],
        ..source
    };
    assert_eq!(
        ambiguous.validate_groups(&mut CoverageBudget::new(100)),
        Err(EncodedImageError::AmbiguousImplicitGroup)
    );
}
