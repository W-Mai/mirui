use super::*;
use crate::image::{ColorDescription, GroupPlanes, Region, SurfaceDescriptor, UnitGroup};
use alloc::vec;

#[test]
fn raw_and_rle_share_logical_rows_padding_and_alignment_for_every_layout() {
    let mut visited = 0;
    for raw in 0..=u16::MAX {
        let layout = SampleLayout::new(raw);
        if !layout.is_known() {
            continue;
        }
        visited += 1;
        let color = if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else if layout.is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(5, 3, layout, color).unwrap();
        let size: usize = surface
            .planes()
            .map(|p| p.minimum_stride().unwrap() as usize * p.height() as usize)
            .sum();
        let mut samples = vec![0; size];
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = (index as u8).wrapping_mul(37).wrapping_add(255);
        }
        let codec = Rle::new();
        let mut encoded = vec![0; codec.encoded_len(&samples).unwrap()];
        codec.encode_into(&samples, &mut encoded).unwrap();
        let raw = UnitGroup::builder(surface, CodingRecord::RAW, &samples)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let rle = UnitGroup::builder(surface, codec.record(), &encoded)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let requirements = SurfaceRequirements::new()
            .with_base_alignment(64)
            .with_plane_alignment(64)
            .with_stride_multiple(64)
            .with_height_multiple(4);
        let raw = raw.decode_plan(requirements).unwrap();
        let rle = rle.decode_plan(requirements).unwrap();
        assert_eq!(raw.memory_plan(), rle.memory_plan());
        #[repr(align(64))]
        struct Aligned([u8; 1024]);
        let mut a = Aligned([0xad; 1024]);
        let mut b = Aligned([0xad; 1024]);
        let decoded = raw.decode_into(&mut a.0).unwrap();
        assert!(decoded.planes().all(|plane| plane.address_is_aligned()));
        rle.decode_into(&mut b.0).unwrap();
        assert_eq!(a.0, b.0, "layout {layout:?}");
    }
    assert!(visited >= 22);
}

#[test]
fn raw_units_retain_plane_coordinates_and_detach_output_lifetime() {
    let mut output = [0xad; 16];
    let decoded = {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let cell = 4u32.to_le_bytes();
        let selection = crate::media::UnitSelection::list(6, &cell).unwrap();
        let data = [10, 20];
        let unit = UnitGroup::builder(surface, CodingRecord::RAW, &data)
            .with_planes(GroupPlanes::Plane(1))
            .with_tiles(1, 1)
            .with_selection(selection)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        unit.decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
            .unwrap()
            .decode_into(&mut output)
            .unwrap()
    };
    assert_eq!(decoded.plane(0), None);
    assert_eq!(
        decoded.plane(1).unwrap().row(0).unwrap(),
        Some(&[10, 20][..])
    );
    assert_eq!(
        decoded.memory_plan().plane(1).unwrap().source_region(),
        Region::new(1, 1, 1, 1).unwrap()
    );
    assert_eq!(&output[2..8], &[0; 6]);
    assert_eq!(&output[8..], &[0xad; 8]);
}

#[test]
fn raw_profile_length_and_output_errors_precede_all_writes() {
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let make = |record, data| {
        UnitGroup::builder(surface, record, data)
            .build()
            .unwrap()
            .get(0)
            .unwrap()
    };
    assert_eq!(CodingRecord::RAW, CodingRecord::new(CodingId::RAW, 1, &[]));
    for revision in [0, 2, u16::MAX] {
        assert_eq!(
            make(CodingRecord::new(CodingId::RAW, revision, &[]), &[1, 2])
                .decode_plan(SurfaceRequirements::new()),
            Err(UnitDecodeError::UnsupportedRevision {
                coding: CodingId::RAW,
                revision
            })
        );
    }
    assert_eq!(
        make(CodingRecord::new(CodingId::RAW, 1, &[0]), &[1, 2])
            .decode_plan(SurfaceRequirements::new()),
        Err(UnitDecodeError::UnexpectedParameters(CodingId::RAW))
    );
    for bytes in [&[1][..], &[1, 2, 3][..]] {
        assert_eq!(
            make(CodingRecord::RAW, bytes).decode_plan(SurfaceRequirements::new()),
            Err(UnitDecodeError::SampleLengthMismatch {
                expected: 2,
                actual: bytes.len()
            })
        );
    }
    let shifted = [0xad, 1, 2];
    let unit = make(CodingRecord::RAW, &shifted[1..]);
    let plan = unit
        .decode_plan(
            SurfaceRequirements::new()
                .with_base_alignment(64)
                .with_stride_multiple(64),
        )
        .unwrap();
    #[repr(align(64))]
    struct Aligned([u8; 128]);
    let mut output = Aligned([0xad; 128]);
    assert!(matches!(
        plan.decode_into(&mut output.0[..63]),
        Err(UnitDecodeError::Output(
            BufferRequirementError::TooSmall { .. }
        ))
    ));
    assert!(matches!(
        plan.decode_into(&mut output.0[1..]),
        Err(UnitDecodeError::Output(
            BufferRequirementError::AddressUnaligned { .. }
        ))
    ));
    assert_eq!(output.0, [0xad; 128]);
    plan.decode_into(&mut output.0).unwrap();
    assert_eq!(&output.0[..2], &[1, 2]);
}
