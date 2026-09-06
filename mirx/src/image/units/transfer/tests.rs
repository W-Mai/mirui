use super::*;
use crate::{
    image::{
        ColorDescription, GroupPlanes, SampleLayout, SurfaceDescriptor, SurfaceRequirements,
        UnitGroup,
    },
    media::{CodingRecord, UnitSelection},
};
use alloc::{vec, vec::Vec};

#[repr(align(64))]
struct Buffer([u8; 4096]);

#[test]
fn every_layout_and_original_plane_places_only_owned_sample_bits() {
    let mut layouts = 0;
    for id in 0..=u16::MAX {
        let layout = SampleLayout::new(id);
        if !layout.is_known() {
            continue;
        }
        layouts += 1;
        let color = if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else if layout.is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(7, 5, layout, color).unwrap();
        let destination = surface
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                    .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64)
                    .with_width_multiple(8)
                    .with_height_multiple(2),
            )
            .unwrap();
        let all = GroupPlanes::Joint((1 << surface.plane_count()) - 1);
        for planes in
            core::iter::once(all).chain((0..surface.plane_count()).map(GroupPlanes::Plane))
        {
            let (width, height, tile_width, tile_height) = match planes {
                GroupPlanes::Plane(index) => {
                    let plane = surface.plane(index).unwrap();
                    (plane.width(), plane.height(), 3, 2)
                }
                _ => (
                    surface.width(),
                    surface.height(),
                    if layout.is_yuv() { 2 } else { 3 },
                    2,
                ),
            };
            let count = width.div_ceil(tile_width) * height.div_ceil(tile_height);
            for cell in [0, 1, count - 1] {
                let index = cell.to_le_bytes();
                let selection = UnitSelection::list(count, &index).unwrap();
                let make = |data| {
                    UnitGroup::builder(surface, CodingRecord::RAW, data)
                        .with_planes(planes)
                        .with_tiles(tile_width, tile_height)
                        .with_selection(selection)
                        .build()
                        .unwrap()
                        .get(0)
                        .unwrap()
                };
                let memory = make(&[0]).memory_plan(SurfaceRequirements::new()).unwrap();
                let samples: Vec<u8> = (0..memory.sample_byte_len())
                    .map(|n| (n as u8).wrapping_mul(37).wrapping_add(0x96))
                    .collect();
                let unit = make(&samples);
                let mut unit_buffer = Buffer([0x5a; 4096]);
                let decoded = unit
                    .decode_plan(
                        SurfaceRequirements::new()
                            .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                            .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                            .with_stride_multiple(16),
                    )
                    .unwrap()
                    .decode_into(&mut unit_buffer.0)
                    .unwrap();
                let mut output = Buffer([0xa5; 4096]);
                let mut expected = output.0;
                let mut input_offset = 0;
                for source in memory.planes() {
                    let region = source.source_region();
                    let geometry = source.geometry();
                    let target = destination.plane(source.index()).unwrap();
                    let row_bytes = geometry.minimum_stride().unwrap() as usize;
                    let bpp = geometry.bits_per_element() as usize;
                    for row in 0..region.height() as usize {
                        for bit in 0..region.width() as usize * bpp {
                            let value = samples[input_offset + row * row_bytes + bit / 8]
                                >> (7 - bit % 8)
                                & 1;
                            let x = region.x() as usize * bpp + bit;
                            let byte = target.data_offset() as usize
                                + (region.y() as usize + row) * target.stride() as usize
                                + x / 8;
                            expected[byte] =
                                (expected[byte] & !(1 << (7 - x % 8))) | (value << (7 - x % 8));
                        }
                    }
                    input_offset += row_bytes * region.height() as usize;
                }
                decoded.copy_into(&mut output.0, destination).unwrap();
                assert_eq!(
                    output.0, expected,
                    "layout={layout:?}, planes={planes:?}, cell={cell}"
                );
                assert_eq!(
                    &output.0[destination.byte_len() as usize..],
                    &vec![0xa5; 4096 - destination.byte_len() as usize]
                );
            }
        }
    }
    assert_eq!(layouts, 22);
}

#[test]
fn placement_errors_preserve_the_complete_target() {
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let source = UnitGroup::builder(surface, CodingRecord::RAW, &[1, 2])
        .build()
        .unwrap();
    let mut unit = [0; 2];
    let decoded = source
        .get(0)
        .unwrap()
        .decode_plan(SurfaceRequirements::new())
        .unwrap()
        .decode_into(&mut unit)
        .unwrap();
    let target = surface
        .memory_plan(
            SurfaceRequirements::new().with_base_alignment(crate::ByteAlignment::new(64).unwrap()),
        )
        .unwrap();
    let mut output = Buffer([0xa5; 4096]);
    assert!(matches!(
        decoded.copy_into(&mut output.0[1..], target),
        Err(SurfaceCopyError::Output(_))
    ));
    assert_eq!(output.0, [0xa5; 4096]);
    assert!(matches!(
        decoded.copy_into(&mut output.0[..1], target),
        Err(SurfaceCopyError::Output(_))
    ));
    assert_eq!(output.0, [0xa5; 4096]);
    let different = SurfaceDescriptor::new(1, 2, SampleLayout::A8, ColorDescription::NONE)
        .unwrap()
        .memory_plan(SurfaceRequirements::new())
        .unwrap();
    assert_eq!(
        decoded.copy_into(&mut output.0, different),
        Err(SurfaceCopyError::SurfaceMismatch)
    );
    assert_eq!(output.0, [0xa5; 4096]);
}
