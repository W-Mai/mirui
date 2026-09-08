use super::*;
use crate::{
    coding::{CodingId, Rle},
    image::{
        CacheSync, ColorDescription, CoverageBudget, DecodeExecution, DecodeRequest,
        DecodeRequestError, EncodedImageAsset, EncodedImageView, MemoryPlacement, SampleLayout,
        SurfaceDescriptor, UnitGroupRecord,
    },
    media::{CodingRecord, MediaSectionKind},
};

#[repr(align(64))]
struct Buffer([u8; 256]);

#[test]
fn explicit_memory_contract_is_checked_before_image_preflight() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42])
        .encode()
        .unwrap();
    let image = EncodedImageView::open(&payload).unwrap();
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    let request = DecodeRequest::new(
        SurfaceRequirements::new()
            .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
            .with_stride_multiple(64),
    )
    .with_input(MemoryPlacement::Flash)
    .with_output(MemoryPlacement::SharedNoncoherent)
    .with_workspace(MemoryPlacement::SharedCoherent)
    .with_workspace_alignment(crate::ByteAlignment::new(64).unwrap());
    let plan = groups
        .decode_plan_for(request, &PayloadLimits::EMBEDDED)
        .unwrap();
    assert_eq!(plan.request(), request);
    assert_eq!(plan.request().input_sync(), CacheSync::None);
    assert_eq!(plan.request().output_sync(), CacheSync::CleanAfterWrite);
    assert_eq!(plan.workspace_requirements().base_alignment(), 64);
    assert_eq!(plan.input_alignment(), 1);
    assert!(plan.input_addresses_are_aligned());
    let mut output = Buffer([0xa5; 256]);
    let mut workspace = Buffer([0x5a; 256]);
    let decoded = plan.decode_into(&mut output.0, &mut workspace.0).unwrap();
    assert_eq!(
        decoded.plane(0).unwrap().row(0).unwrap(),
        Some(&[42; 4][..])
    );

    for (request, expected) in [
        (
            DecodeRequest::default().with_execution(DecodeExecution::Compute),
            DecodeRequestError::UnsupportedExecution(DecodeExecution::Compute),
        ),
        (
            DecodeRequest::default().with_execution(DecodeExecution::DirectUpload),
            DecodeRequestError::UnsupportedExecution(DecodeExecution::DirectUpload),
        ),
        (
            DecodeRequest::default().with_input(MemoryPlacement::Device),
            DecodeRequestError::InputNotReadable(MemoryPlacement::Device),
        ),
        (
            DecodeRequest::default().with_output(MemoryPlacement::Flash),
            DecodeRequestError::OutputNotWritable(MemoryPlacement::Flash),
        ),
        (
            DecodeRequest::default().with_workspace(MemoryPlacement::Device),
            DecodeRequestError::WorkspaceNotWritable(MemoryPlacement::Device),
        ),
    ] {
        assert_eq!(
            groups
                .decode_plan_for(request, &PayloadLimits::EMBEDDED)
                .unwrap_err(),
            DecodeError::Request(expected)
        );
    }
}

#[test]
fn empty_surfaces_omit_units_and_need_no_output_or_workspace() {
    for (width, height) in [(0, 0), (0, u32::MAX), (u32::MAX, 0)] {
        let surface =
            SurfaceDescriptor::new(width, height, SampleLayout::A1, ColorDescription::NONE)
                .unwrap();
        let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[])
            .encode()
            .unwrap();
        let image = EncodedImageView::open(&payload).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        let plan = groups
            .decode_plan(SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
            .unwrap();
        assert_eq!(plan.unit_count(), 0);
        assert_eq!(plan.memory_plan().byte_len(), 0);
        assert_eq!(plan.workspace_requirements().byte_len(), 0);
        let mut output = [0xa5; 3];
        let mut workspace = [0x5a; 3];
        assert_eq!(
            plan.decode_into(&mut output, &mut workspace)
                .unwrap()
                .surface(),
            surface
        );
        assert_eq!(output, [0xa5; 3]);
        assert_eq!(workspace, [0x5a; 3]);
    }
}

#[test]
fn complete_sub_byte_tiles_share_one_byte_workspace_and_exact_work_limits() {
    let surface = SurfaceDescriptor::new(9, 2, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let coding = [Rle::new().record()];
    let groups = [UnitGroupRecord::new(0, 0..12).unwrap().with_tiles(3, 1)];
    let data = [
        0x80, 0xe0, 0x80, 0xa0, 0x80, 0x40, 0x80, 0x20, 0x80, 0x60, 0x80, 0xc0,
    ];
    let payload = EncodedImageAsset::from_groups(surface, &coding, &groups, &data)
        .encode()
        .unwrap();
    let image = EncodedImageView::open(&payload).unwrap();
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(1000))
        .unwrap();
    let requirements = SurfaceRequirements::new()
        .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
        .with_stride_multiple(64);
    let plan = groups
        .decode_plan(requirements, &PayloadLimits::EMBEDDED)
        .unwrap();
    assert_eq!(plan.unit_count(), 6);
    assert_eq!(plan.memory_plan().byte_len(), 128);
    assert_eq!(plan.workspace_requirements().byte_len(), 1);
    assert_eq!(plan.workspace_requirements().base_alignment(), 1);
    groups
        .decode_plan(
            requirements,
            &PayloadLimits::EMBEDDED.with_max_raster_work(plan.work()),
        )
        .unwrap();
    assert!(
        groups
            .decode_plan(
                requirements,
                &PayloadLimits::EMBEDDED.with_max_raster_work(plan.work() - 1)
            )
            .is_err()
    );
    for limits in [
        PayloadLimits::EMBEDDED.with_max_raster_units(5),
        PayloadLimits::EMBEDDED.with_max_raster_groups(0),
        PayloadLimits::EMBEDDED.with_max_decoded_bytes(0),
    ] {
        assert!(groups.decode_plan(requirements, &limits).is_err());
    }
    let mut output = Buffer([0xa5; 256]);
    let mut workspace = [0x5a; 8];
    let view = plan.decode_into(&mut output.0, &mut workspace).unwrap();
    assert_eq!(view.surface(), surface);
    assert!(view.data_addresses_are_aligned());
    let plane = view.plane(0).unwrap();
    assert_eq!(plane.row(0).unwrap(), Some(&[0xf5, 0][..]));
    assert_eq!(plane.row(1).unwrap(), Some(&[0x2f, 0][..]));
    assert_eq!(&output.0[2..64], &[0; 62]);
    assert_eq!(&output.0[66..128], &[0; 62]);
    assert_eq!(&output.0[128..], &[0xa5; 128]);
    assert_eq!(&workspace[1..], &[0x5a; 7]);
}

#[test]
fn all_input_and_binding_failures_precede_final_writes() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    for (record, data) in [
        (CodingRecord::new(CodingId::new(511), 1, &[]), &[1][..]),
        (Rle::new().record(), &[0xff][..]),
    ] {
        let payload = EncodedImageAsset::new(surface, record, data)
            .encode()
            .unwrap();
        let image = EncodedImageView::open(&payload).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        assert!(
            groups
                .decode_plan(SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
                .is_err()
        );
    }
    let mut payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42])
        .encode()
        .unwrap();
    let image = EncodedImageView::open(&payload).unwrap();
    let offset = image
        .media()
        .section(MediaSectionKind::DATA)
        .unwrap()
        .descriptor()
        .offset() as usize;
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(1000))
        .unwrap();
    let plan = groups
        .decode_plan(
            SurfaceRequirements::new().with_base_alignment(crate::ByteAlignment::new(64).unwrap()),
            &PayloadLimits::EMBEDDED,
        )
        .unwrap();
    assert_eq!(plan.workspace_requirements().byte_len(), 4);
    let mut output = Buffer([0xa5; 256]);
    let mut workspace = [0x5a; 8];
    assert!(matches!(
        plan.decode_into(&mut output.0, &mut workspace[..3]),
        Err(DecodeError::Workspace(_))
    ));
    assert!(matches!(
        plan.decode_into(&mut output.0[..3], &mut workspace),
        Err(DecodeError::Output(_))
    ));
    assert!(matches!(
        plan.decode_into(&mut output.0[1..], &mut workspace),
        Err(DecodeError::Output(_))
    ));
    assert_eq!(output.0, [0xa5; 256]);
    assert_eq!(workspace, [0x5a; 8]);
    payload[offset + 1] ^= 1;
    let image = EncodedImageView::open(&payload).unwrap();
    let mut slots = [None];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(1000))
        .unwrap();
    assert!(matches!(
        groups.decode_plan(SurfaceRequirements::new(), &PayloadLimits::EMBEDDED),
        Err(DecodeError::Image(EncodedImageError::Media(_)))
    ));
}
