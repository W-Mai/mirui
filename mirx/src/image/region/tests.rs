use super::*;
use crate::{
    image::{
        ColorDescription, PlaneMemoryFlags, PlaneMemoryLayout, RawImageAsset, RegionError,
        SampleLayout, SurfaceCopyError, SurfaceFlags, UnitGroup,
    },
    media::{CodingRecord, UnitSelection},
};
use alloc::{vec, vec::Vec};

#[repr(align(64))]
struct Buffer([u8; 1024]);

#[test]
fn all_layouts_preserve_exact_samples_metadata_and_aligned_crop_geometry() {
    let mut layouts = 0;
    for id in 0..=u16::MAX {
        let layout = SampleLayout::new(id);
        if !layout.is_known() {
            continue;
        }
        let color = if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else if layout.is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(9, 5, layout, color)
            .unwrap()
            .with_flags(SurfaceFlags::from_bits_retain(0x8000))
            .with_pixel_aspect(4, 3)
            .unwrap();
        let buffers: Vec<Vec<u8>> = surface
            .planes()
            .enumerate()
            .map(|(index, plane)| {
                (0..plane.minimum_stride().unwrap() * plane.height())
                    .map(|n| {
                        (n as u8)
                            .wrapping_mul(59)
                            .wrapping_add(index as u8 * 31 + 0x97)
                    })
                    .collect()
            })
            .collect();
        let planes: Vec<_> = buffers.iter().map(Vec::as_slice).collect();
        let samples: Vec<_> = buffers.iter().flatten().copied().collect();
        let palette = vec![0; layout.color_table_entries().unwrap_or(0) as usize * 4];
        let mut asset = RawImageAsset::new(surface, &planes);
        if !palette.is_empty() {
            asset = asset.with_color_table(&palette);
        }
        let source = asset.view().unwrap();
        let region = if layout.is_yuv() {
            surface.region(2, 2, 7, 3).unwrap()
        } else {
            surface.region(3, 1, 5, 3).unwrap()
        };
        let plan = surface
            .region_plan(
                region,
                SurfaceRequirements::new()
                    .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                    .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64)
                    .with_width_multiple(4)
                    .with_height_multiple(4),
            )
            .unwrap();
        let memory = plan.memory_plan();
        assert_eq!(
            memory.surface(),
            surface.with_extent(region.width(), region.height())
        );
        let mut expected = Buffer([0xa5; 1024]);
        expected.0[..memory.byte_len() as usize].fill(0);
        for (index, plane) in source.planes().enumerate() {
            let crop = plan.plane_region(index as u8).unwrap();
            let target = memory.plane(index as u8).unwrap();
            let bpp = u64::from(plane.geometry().bits_per_element());
            for y in 0..crop.height() {
                let row = plane.row(y + crop.y()).unwrap().unwrap();
                for bit in 0..u64::from(crop.width()) * bpp {
                    let s = (u64::from(crop.x()) * bpp + bit) as usize;
                    let t = target.data_offset() as usize
                        + y as usize * target.stride() as usize
                        + bit as usize / 8;
                    expected.0[t] |= (row[s / 8] >> (7 - s % 8) & 1) << (7 - bit % 8);
                }
            }
        }
        let mut output = Buffer([0xa5; 1024]);
        let copied = source.copy_region_into(&mut output.0, plan).unwrap();
        assert!(copied.data_addresses_are_aligned());
        assert_eq!(copied.color_table(), source.color_table());
        assert_eq!(output.0, expected.0, "RAW {layout:?}");
        let group = UnitGroup::builder(surface, CodingRecord::RAW, &samples)
            .build()
            .unwrap();
        let unit = group
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new())
            .unwrap();
        let mut workspace = vec![0; unit.memory_plan().byte_len() as usize];
        let decoded = unit.decode_into(&mut workspace).unwrap();
        let mut output = Buffer([0xa5; 1024]);
        output.0[..memory.byte_len() as usize].fill(0);
        decoded.copy_region_into(&mut output.0, plan).unwrap();
        assert_eq!(output.0, expected.0, "unit {layout:?}");
        layouts += 1;
    }
    assert_eq!(layouts, 22);
}

#[test]
fn clipped_sub_byte_units_preserve_neighbours_and_skip_nonintersections() {
    let surface = SurfaceDescriptor::new(9, 1, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let cells = 1u32.to_le_bytes();
    let selection = UnitSelection::list(3, &cells).unwrap();
    let group = UnitGroup::builder(surface, CodingRecord::RAW, &[0xe0])
        .with_tiles(3, 1)
        .with_selection(selection)
        .build()
        .unwrap();
    let mut workspace = [0; 1];
    let unit = group
        .get(0)
        .unwrap()
        .decode_plan(SurfaceRequirements::new())
        .unwrap();
    let decoded = unit.decode_into(&mut workspace).unwrap();
    for (x, width, expected) in [(2, 6, 0x7a), (4, 4, 0xda), (5, 4, 0xda), (0, 3, 0x5a)] {
        let plan = surface
            .region_plan(
                surface.region(x, 0, width, 1).unwrap(),
                SurfaceRequirements::new().with_stride_multiple(8),
            )
            .unwrap();
        let mut output = [0x5a; 16];
        decoded.copy_region_into(&mut output, plan).unwrap();
        assert_eq!(output[0], expected, "x={x}");
        assert_eq!(&output[1..], &[0x5a; 15]);
    }
}

#[test]
fn region_and_buffer_errors_never_change_output() {
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    for region in [
        Region::new(1, 0, 2, 2).unwrap(),
        Region::new(0, 0, 3, 2).unwrap(),
        Region::new(0, 1, 2, 2).unwrap(),
        Region::new(0, 0, 2, 1).unwrap(),
    ] {
        assert!(matches!(
            surface.region_plan(region, SurfaceRequirements::new()),
            Err(SurfacePlanError::Region(
                RegionError::UnalignedPlaneRegion { .. }
            ))
        ));
    }
    assert!(matches!(
        surface.region_plan(Region::new(4, 0, 2, 2).unwrap(), SurfaceRequirements::new()),
        Err(SurfacePlanError::Region(RegionError::OutOfBounds))
    ));
    assert!(
        SurfaceDescriptor::new(0, 0, SampleLayout::new(u16::MAX), ColorDescription::NONE).is_err()
    );
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let source = RawImageAsset::new(surface, &[&[1, 2, 3, 4]])
        .view()
        .unwrap();
    let plan = surface
        .region_plan(
            surface.region(1, 0, 2, 1).unwrap(),
            SurfaceRequirements::new().with_base_alignment(crate::ByteAlignment::new(64).unwrap()),
        )
        .unwrap();
    let mut output = Buffer([0xa5; 1024]);
    assert!(matches!(
        source.copy_region_into(&mut output.0[..1], plan),
        Err(SurfaceCopyError::Output(_))
    ));
    assert!(matches!(
        source.copy_region_into(&mut output.0[1..], plan),
        Err(SurfaceCopyError::Output(_))
    ));
    let other = surface
        .with_extent(5, 1)
        .region_plan(Region::new(1, 0, 2, 1).unwrap(), SurfaceRequirements::new())
        .unwrap();
    assert_eq!(
        source.copy_region_into(&mut output.0, other),
        Err(SurfaceCopyError::SurfaceMismatch)
    );
    let layout = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
        .with_flags(PlaneMemoryFlags::from_bits_retain(0x80))
        .build()
        .unwrap();
    let layouts = [layout];
    let source = RawImageAsset::new(surface, &[&[1, 2, 3, 4]])
        .with_memory_layouts(&layouts)
        .view()
        .unwrap();
    assert!(matches!(
        source.copy_region_into(&mut output.0, plan),
        Err(SurfaceCopyError::UnsupportedPlaneFlags { .. })
    ));
    assert_eq!(output.0, [0xa5; 1024]);
}

#[test]
fn empty_extents_do_not_visit_rows_or_require_storage() {
    for (width, height) in [(0, u32::MAX), (u32::MAX, 0)] {
        let surface =
            SurfaceDescriptor::new(width, height, SampleLayout::A1, ColorDescription::NONE)
                .unwrap();
        let plan = surface
            .region_plan(
                surface.region(0, 0, width, height).unwrap(),
                SurfaceRequirements::new(),
            )
            .unwrap();
        let source = RawImageAsset::new(surface, &[&[]]).view().unwrap();
        let mut output = [0xa5; 8];
        assert_eq!(plan.memory_plan().byte_len(), 0);
        source.copy_region_into(&mut output, plan).unwrap();
        assert_eq!(output, [0xa5; 8]);
    }
}
