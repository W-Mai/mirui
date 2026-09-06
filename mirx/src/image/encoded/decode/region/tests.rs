use super::*;
use crate::{
    coding::Rle,
    image::{
        ColorDescription, CoverageBudget, EncodedImageAsset, GroupSelection, SampleLayout,
        SurfaceDescriptor, UnitGroupRecord,
    },
    media::{CodingId, CodingRecord, DataIntegrity, MediaSectionKind},
};

#[repr(align(64))]
struct Buffer([u8; 512]);

#[test]
fn exact_sub_byte_regions_bound_staging_and_deduplicate_checksum_partitions() {
    let surface = SurfaceDescriptor::new(9, 2, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let coding = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..12).unwrap().with_tiles(3, 1)];
    let data = [
        0x80, 0xe0, 0x80, 0xa0, 0x80, 0x40, 0x80, 0x20, 0x80, 0x60, 0x80, 0xc0,
    ];
    for (integrity, checksum_bytes) in [
        (DataIntegrity::Whole, 12),
        (DataIntegrity::Indexed(&[12]), 12),
        (DataIntegrity::Indexed(&[4, 8, 12]), 8),
    ] {
        let payload = EncodedImageAsset::from_groups(surface, &coding, &records, &data)
            .with_integrity(integrity)
            .encode()
            .unwrap();
        let image = EncodedImageView::open(&payload).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let requested = surface.region(4, 0, 1, 2).unwrap();
        let requirements = SurfaceRequirements::new()
            .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
            .with_stride_multiple(64);
        let plan = groups
            .decode_region_plan(requested, requirements, &PayloadLimits::EMBEDDED)
            .unwrap();
        assert_eq!(plan.region_plan().region(), requested);
        assert_eq!(plan.unit_count(), 2);
        assert_eq!(plan.input_byte_len(), 4);
        assert_eq!(plan.checksum_byte_len(), checksum_bytes);
        assert_eq!(plan.workspace_requirements().byte_len(), 1);
        assert_eq!(plan.memory_plan().byte_len(), 128);
        groups
            .decode_region_plan(
                requested,
                requirements,
                &PayloadLimits::EMBEDDED.with_max_raster_work(plan.work()),
            )
            .unwrap();
        for limits in [
            PayloadLimits::EMBEDDED.with_max_raster_work(plan.work() - 1),
            PayloadLimits::EMBEDDED.with_max_raster_units(1),
            PayloadLimits::EMBEDDED.with_max_decoded_bytes(0),
        ] {
            assert!(
                groups
                    .decode_region_plan(requested, requirements, &limits)
                    .is_err()
            );
        }
        let mut output = Buffer([0xa5; 512]);
        let mut workspace = [0x5a; 4];
        assert!(matches!(
            plan.decode_into(&mut output.0, &mut []),
            Err(DecodeError::Workspace(_))
        ));
        assert!(matches!(
            plan.decode_into(&mut output.0[..127], &mut workspace),
            Err(DecodeError::Output(_))
        ));
        assert!(matches!(
            plan.decode_into(&mut output.0[1..], &mut workspace),
            Err(DecodeError::Output(_))
        ));
        assert_eq!(output.0, [0xa5; 512]);
        assert_eq!(workspace, [0x5a; 4]);
        let view = plan.decode_into(&mut output.0, &mut workspace).unwrap();
        assert_eq!(view.surface().width(), 1);
        assert_eq!(view.plane(0).unwrap().row(0).unwrap(), Some(&[0][..]));
        assert_eq!(view.plane(0).unwrap().row(1).unwrap(), Some(&[0x80][..]));
        assert_eq!(&output.0[1..64], &[0; 63]);
        assert_eq!(&output.0[65..128], &[0; 63]);
        assert_eq!(&output.0[128..], &[0xa5; 384]);
        assert_eq!(&workspace[1..], &[0x5a; 3]);
    }
}

#[test]
fn planar_yuv_queries_project_chroma_and_preserve_odd_outer_edges() {
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let coding = [Rle::new().record()];
    let records = [
        UnitGroupRecord::new(0, 0..12)
            .unwrap()
            .with_planes(GroupPlanes::Plane(0))
            .with_tiles(2, 2),
        UnitGroupRecord::new(0, 12..24)
            .unwrap()
            .with_planes(GroupPlanes::Plane(1))
            .with_tiles(1, 1),
    ];
    let data = [
        0x83, 1, 0x83, 2, 0x81, 3, 0x81, 4, 0x81, 5, 0x80, 6, 0x81, 80, 0x81, 81, 0x81, 82, 0x81,
        83, 0x81, 84, 0x81, 85,
    ];
    let payload = EncodedImageAsset::from_groups(surface, &coding, &records, &data)
        .with_integrity(DataIntegrity::Indexed(&[6, 12, 18, 24]))
        .encode()
        .unwrap();
    let image = EncodedImageView::open(&payload).unwrap();
    let mut slots = [None; 2];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(1000))
        .unwrap();
    let plan = groups
        .decode_region_plan(
            surface.region(2, 0, 3, 3).unwrap(),
            SurfaceRequirements::new()
                .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
            &PayloadLimits::EMBEDDED,
        )
        .unwrap();
    assert_eq!(plan.unit_count(), 8);
    assert_eq!(plan.input_byte_len(), 16);
    assert_eq!(plan.checksum_byte_len(), 24);
    assert_eq!(plan.workspace_requirements().byte_len(), 4);
    assert_eq!(plan.memory_plan().byte_len(), 320);
    let mut output = Buffer([0xa5; 512]);
    let mut workspace = [0; 4];
    let view = plan.decode_into(&mut output.0, &mut workspace).unwrap();
    let y = view.plane(0).unwrap();
    assert_eq!(y.row(0).unwrap(), Some(&[2, 2, 3][..]));
    assert_eq!(y.row(1).unwrap(), Some(&[2, 2, 3][..]));
    assert_eq!(y.row(2).unwrap(), Some(&[5, 5, 6][..]));
    let uv = view.plane(1).unwrap();
    assert_eq!(uv.row(0).unwrap(), Some(&[81, 81, 82, 82][..]));
    assert_eq!(uv.row(1).unwrap(), Some(&[84, 84, 85, 85][..]));
    assert!(
        groups
            .decode_region_plan(
                surface.region(2, 0, 1, 2).unwrap(),
                SurfaceRequirements::new(),
                &PayloadLimits::EMBEDDED
            )
            .is_err()
    );
}

#[test]
fn unselected_syntax_is_not_decoded_but_declared_checksum_expansion_remains_required() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let records = [
        UnitGroupRecord::new(0, 0..2)
            .unwrap()
            .with_tiles(2, 1)
            .with_selection(GroupSelection::List(1)),
        UnitGroupRecord::new(1, 2..4)
            .unwrap()
            .with_tiles(2, 1)
            .with_selection(GroupSelection::List(1))
            .with_index_offset(4),
    ];
    for other in [
        Rle::new().record(),
        CodingRecord::new(CodingId::new(511), 1, &[]),
    ] {
        let coding = [Rle::new().record(), other];
        for integrity in [DataIntegrity::Whole, DataIntegrity::Indexed(&[2, 4])] {
            let mut payload =
                EncodedImageAsset::from_groups(surface, &coding, &records, &[0x81, 42, 0xff, 0])
                    .with_unit_index(&[0, 0, 0, 0, 1, 0, 0, 0])
                    .with_integrity(integrity)
                    .encode()
                    .unwrap();
            let image = EncodedImageView::open(&payload).unwrap();
            let mut slots = [None; 2];
            let groups = image
                .groups_into(&mut slots, &mut CoverageBudget::new(1000))
                .unwrap();
            assert!(
                groups
                    .decode_plan(SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                    .is_err()
            );
            let requested = surface.region(1, 0, 1, 1).unwrap();
            let plan = groups
                .decode_region_plan(
                    requested,
                    SurfaceRequirements::new(),
                    &PayloadLimits::EMBEDDED,
                )
                .unwrap();
            assert_eq!(plan.unit_count(), 1);
            assert_eq!(plan.workspace_requirements().byte_len(), 2);
            let mut out = [0; 1];
            let mut workspace = [0; 2];
            let view = plan.decode_into(&mut out, &mut workspace).unwrap();
            assert_eq!(view.plane(0).unwrap().bytes(), &[42]);
            let start = image
                .media()
                .section(MediaSectionKind::DATA)
                .unwrap()
                .descriptor()
                .offset() as usize;
            payload[start + 3] ^= 1;
            let image = EncodedImageView::open(&payload).unwrap();
            let mut slots = [None; 2];
            let groups = image
                .groups_into(&mut slots, &mut CoverageBudget::new(1000))
                .unwrap();
            assert_eq!(
                groups
                    .decode_region_plan(
                        requested,
                        SurfaceRequirements::new(),
                        &PayloadLimits::EMBEDDED
                    )
                    .is_ok(),
                integrity != DataIntegrity::Whole
            );
            let empty = groups
                .decode_region_plan(
                    surface.region(4, 0, 0, 1).unwrap(),
                    SurfaceRequirements::new(),
                    &PayloadLimits::EMBEDDED,
                )
                .unwrap();
            assert_eq!(empty.unit_count(), 0);
            assert_eq!(empty.input_byte_len(), 0);
            assert_eq!(empty.checksum_byte_len(), 0);
            assert_eq!(empty.workspace_requirements().byte_len(), 0);
            let mut out = [0xa5; 8];
            empty.decode_into(&mut out, &mut []).unwrap();
            assert_eq!(out, [0xa5; 8]);
        }
    }
}
