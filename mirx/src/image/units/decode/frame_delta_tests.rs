use alloc::{vec, vec::Vec};

use super::*;
use crate::{
    ColorFormat,
    coding::FrameDelta,
    image::{ColorDescription, RawImageAsset, SurfaceDescriptor, UnitGroup},
};

#[test]
fn previous_surface_reconstructs_every_layout_into_strided_unit_storage() {
    let layouts = [
        ColorFormat::I1,
        ColorFormat::I2,
        ColorFormat::I4,
        ColorFormat::I8,
        ColorFormat::A1,
        ColorFormat::A2,
        ColorFormat::A4,
        ColorFormat::A8,
        ColorFormat::L8,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB565A8,
        ColorFormat::RGB888,
        ColorFormat::XRGB8888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ]
    .into_iter()
    .map(SampleLayout::from_color_format)
    .chain([
        SampleLayout::I420,
        SampleLayout::YV12,
        SampleLayout::NV12,
        SampleLayout::NV21,
        SampleLayout::P010,
        SampleLayout::P016,
    ]);
    let codec = FrameDelta::new();
    for layout in layouts {
        let color = if layout.is_alpha() {
            ColorDescription::NONE
        } else if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(5, 3, layout, color).unwrap();
        let mut reference_planes = Vec::new();
        let mut current_planes = Vec::new();
        for plane in surface.planes() {
            let row_len = plane.minimum_stride().unwrap() as usize;
            let mut reference = vec![0; row_len * plane.height() as usize];
            let mut current = vec![0; reference.len()];
            for (index, byte) in reference.iter_mut().enumerate() {
                *byte = (index as u8).wrapping_mul(29).wrapping_add(17);
            }
            for (index, byte) in current.iter_mut().enumerate() {
                *byte = reference[index].wrapping_add(if index % 5 == 0 { 7 } else { 0 });
            }
            let tail = plane.row_tail_mask();
            if row_len != 0 && tail != 0xff {
                for row in 0..plane.height() as usize {
                    reference[row * row_len + row_len - 1] &= tail;
                    current[row * row_len + row_len - 1] &= tail;
                }
            }
            reference_planes.push(reference);
            current_planes.push(current);
        }
        let reference_tight: Vec<_> = reference_planes.iter().flatten().copied().collect();
        let current_tight: Vec<_> = current_planes.iter().flatten().copied().collect();
        let mut encoded = vec![0xa5; codec.encoded_bound(current_tight.len()).unwrap()];
        let encoded_len = codec
            .encode_into(&reference_tight, &current_tight, &mut encoded)
            .unwrap();
        let group = UnitGroup::builder(surface, codec.record(), &encoded[..encoded_len])
            .with_reference(ReferenceMode::Previous)
            .build()
            .unwrap();
        let unit = group.get(0).unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
            .unwrap();
        let reference_slices: Vec<_> = reference_planes.iter().map(Vec::as_slice).collect();
        let palette = [0; 1024];
        let mut reference = RawImageAsset::new(surface, &reference_slices);
        if let Some(entries) = layout.color_table_entries() {
            reference = reference.with_color_table(&palette[..entries as usize * 4]);
        }
        let reference = reference.view().unwrap();
        let mut output = vec![0xa5; plan.memory_plan().byte_len() as usize + 3];
        let decoded = plan.decode_from(reference, &mut output).unwrap();
        for (index, plane) in decoded.planes().enumerate() {
            let expected = &current_planes[index];
            let row_len = plane.geometry().minimum_stride().unwrap() as usize;
            for row in 0..plane.geometry().height() as usize {
                assert_eq!(
                    plane.row(row as u32).unwrap().unwrap(),
                    &expected[row * row_len..(row + 1) * row_len],
                    "{layout:?}, plane {index}, row {row}"
                );
            }
        }
        assert!(
            output[plan.memory_plan().byte_len() as usize..]
                .iter()
                .all(|&byte| byte == 0xa5)
        );
    }
}

#[test]
fn independent_delta_and_reference_free_execution_are_rejected_before_writes() {
    let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let independent = UnitGroup::builder(surface, FrameDelta::new().record(), &[0])
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    assert!(matches!(
        independent.decode_plan(SurfaceRequirements::new()),
        Err(UnitDecodeError::PreviousReferenceRequired(
            CodingId::FRAME_DELTA
        ))
    ));

    let dependent = UnitGroup::builder(surface, FrameDelta::new().record(), &[0])
        .with_reference(ReferenceMode::Previous)
        .build()
        .unwrap()
        .get(0)
        .unwrap()
        .decode_plan(SurfaceRequirements::new())
        .unwrap();
    let mut output = [0xa5];
    assert!(matches!(
        dependent.decode_into(&mut output),
        Err(UnitDecodeError::ReferenceRequired(CodingId::FRAME_DELTA))
    ));
    assert_eq!(output, [0xa5]);
}
