use super::*;
use crate::image::{
    ColorDescription, GroupPlanes, Region, SampleLayout, SurfaceDescriptor, UnitGroup,
};
use crate::media::CodingRecord;
use alloc::vec;

fn surface(width: u32, height: u32, layout: SampleLayout) -> SurfaceDescriptor {
    SurfaceDescriptor::new(width, height, layout, ColorDescription::SRGB).unwrap()
}

#[test]
fn pixel_runs_cross_rows_without_touching_stride_or_allocation_padding() {
    let surface = surface(20, 3, SampleLayout::RGBA8888);
    let codec = Pixel::new(surface.sample_layout()).unwrap();
    let mut samples = [0; 240];
    for value in samples.chunks_exact_mut(4) {
        value.copy_from_slice(&[11, 22, 33, 44]);
    }
    let mut encoded = [0; 32];
    let len = codec.encode_into(&samples, &mut encoded).unwrap();
    let unit = UnitGroup::builder(surface, codec.record(), &encoded[..len])
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let plan = unit
        .decode_plan(
            SurfaceRequirements::new()
                .with_base_alignment(64)
                .with_plane_alignment(64)
                .with_stride_multiple(64)
                .with_height_multiple(4),
        )
        .unwrap();
    assert_eq!(plan.memory_plan().source_surface(), surface);
    let memory = plan.memory_plan().plane(0).unwrap().memory();
    assert_eq!(memory.allocation_width(), 20);
    assert_eq!(memory.allocation_height(), 4);
    assert_eq!(memory.stride(), 128);
    #[repr(align(64))]
    struct Buffer([u8; 576]);
    let mut output = Buffer([0xad; 576]);
    let decoded = plan.decode_into(&mut output.0).unwrap();
    let plane = decoded.plane(0).unwrap();
    assert!(plane.address_is_aligned());
    assert_eq!(plane.geometry().width(), 20);
    assert_eq!(plane.geometry().height(), 3);
    for (row, expected) in plane.rows().unwrap().zip(samples.chunks_exact(80)) {
        assert_eq!(row, expected);
    }
    for row in 0..3 {
        assert!(
            plane.bytes()[row * 128 + 80..(row + 1) * 128]
                .iter()
                .all(|b| *b == 0)
        );
    }
    assert!(plane.bytes()[384..512].iter().all(|b| *b == 0));
    assert_eq!(decoded.planes().len(), 1);
    assert_eq!(decoded.planes().next_back(), Some(plane));
    assert!(decoded.plane(1).is_none());
    assert_eq!(&output.0[512..], &[0xad; 64]);
}

#[test]
fn edge_unit_output_retains_origin_and_outlives_encoded_storage() {
    let source = surface(10, 3, SampleLayout::RGB888);
    let codec = Pixel::new(source.sample_layout()).unwrap();
    let mut output = [0xad; 12];
    let decoded = {
        let input = vec![7, 7, 3, 3, 3, 1];
        let unit = UnitGroup::builder(source, codec.record(), &input)
            .with_tiles(4, 2)
            .with_planes(GroupPlanes::Plane(0))
            .build()
            .unwrap()
            .get(5)
            .unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
            .unwrap();
        plan.decode_into(&mut output).unwrap()
    };
    let descriptor = decoded.memory_plan().plane(0).unwrap();
    assert_eq!(descriptor.source_region(), Region::new(8, 2, 2, 1).unwrap());
    assert_eq!(descriptor.geometry().width(), 2);
    let plane = decoded.plane(0).unwrap();
    assert_eq!(plane.row(0).unwrap(), Some(&[0; 6][..]));
    assert_eq!(plane.row(1).unwrap(), None);
    assert_eq!(plane.bytes(), &[0; 8]);
    assert_eq!(&output[8..], &[0xad; 4]);
}

#[test]
fn invalid_profiles_streams_and_output_addresses_cannot_modify_destinations() {
    let source = surface(1, 1, SampleLayout::RGB888);
    let codec = Pixel::new(source.sample_layout()).unwrap();
    let make = |coding| {
        UnitGroup::builder(source, coding, &[0])
            .build()
            .unwrap()
            .get(0)
            .unwrap()
    };
    let requirements = SurfaceRequirements::new();
    assert!(matches!(
        make(CodingRecord::new(CodingId::RAW, 1, &[])).decode_plan(requirements),
        Err(UnitDecodeError::UnsupportedCoding(CodingId::RAW))
    ));
    assert!(matches!(
        make(CodingRecord::new(CodingId::PIXEL, 2, &[])).decode_plan(requirements),
        Err(UnitDecodeError::Pixel(PixelError::UnsupportedRevision(2)))
    ));
    let previous = UnitGroup::builder(source, codec.record(), &[0])
        .with_reference(ReferenceMode::Previous)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    assert_eq!(
        previous.decode_plan(requirements),
        Err(UnitDecodeError::UnsupportedReference(
            ReferenceMode::Previous
        ))
    );
    let invalid = UnitGroup::builder(source, codec.record(), &[1])
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    assert!(matches!(
        invalid.decode_plan(requirements),
        Err(UnitDecodeError::Pixel(PixelError::RunOverflow { .. }))
    ));
    let unit = make(codec.record());
    let plan = unit
        .decode_plan(
            requirements
                .with_base_alignment(64)
                .with_stride_multiple(64),
        )
        .unwrap();
    #[repr(align(64))]
    struct Buffer([u8; 128]);
    let mut output = Buffer([0xad; 128]);
    assert!(matches!(
        plan.decode_into(&mut output.0[1..]),
        Err(UnitDecodeError::Output(
            BufferRequirementError::AddressUnaligned { .. }
        ))
    ));
    assert!(matches!(
        plan.decode_into(&mut output.0[..63]),
        Err(UnitDecodeError::Output(
            BufferRequirementError::TooSmall { .. }
        ))
    ));
    assert_eq!(output.0, [0xad; 128]);
}

#[test]
fn bytewise_scalar_input_and_output_alignment_are_independent() {
    let source = surface(2, 1, SampleLayout::RGB888);
    let codec = Pixel::new(source.sample_layout()).unwrap();
    #[repr(align(64))]
    struct Input([u8; 64]);
    let input = Input([1; 64]);
    let unit = UnitGroup::builder(source, codec.record(), &input.0[1..2])
        .with_input_alignment(64)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    assert!(!unit.data_address_is_aligned());
    let plan = unit.decode_plan(SurfaceRequirements::new()).unwrap();
    assert_eq!(plan.memory_plan().base_alignment(), 1);
    let mut output = [0xad; 7];
    plan.decode_into(&mut output[1..]).unwrap();
    assert_eq!(output, [0xad, 0, 0, 0, 0, 0, 0]);
}
