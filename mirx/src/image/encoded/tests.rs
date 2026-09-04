use super::*;
use crate::image::{ColorDescription, GroupSelection, SampleLayout};
use crate::media::{
    CodingId, CodingRecord, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaSectionFlags,
};
use crate::wire::{write_u16_le, write_u32_le};
use alloc::{vec, vec::Vec};

fn surface() -> SurfaceDescriptor {
    SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap()
}

#[test]
fn encoded_metadata_integrity_and_strided_pixel_execution_share_unit_geometry() {
    use crate::coding::Pixel;
    use crate::image::SurfaceRequirements;

    let surface =
        SurfaceDescriptor::new(3, 2, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let codec = Pixel::new(surface.sample_layout()).unwrap();
    for indexed in [false, true] {
        let mut bytes = payload(
            surface,
            &[codec.record()],
            None,
            None,
            &[5],
            None,
            if indexed { Some(&[1]) } else { None },
        );
        let view = EncodedImageView::open(&bytes).unwrap();
        let mut slots = [None];
        let groups = view
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert_eq!(groups.validate_unit(0, 0), Ok(1));
        let unit = groups.get(0).unwrap().get(0).unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(16))
            .unwrap();
        let mut output = [0xad; 40];
        let decoded = plan.decode_into(&mut output).unwrap();
        assert_eq!(decoded.plane(0).unwrap().memory().stride(), 16);
        assert_eq!(decoded.plane(0).unwrap().row(1).unwrap(), Some(&[0; 9][..]));
        assert_eq!(&output[..32], &[0; 32]);
        assert_eq!(&output[32..], &[0xad; 8]);

        let data = view
            .media()
            .sections()
            .find(|section| section.descriptor().kind() == MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset() as usize;
        bytes[data] ^= 1;
        // Metadata opening and group preparation do not scan DATA implicitly.
        let view = EncodedImageView::open(&bytes).unwrap();
        let mut slots = [None];
        let groups = view
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert!(groups.validate_unit(0, 0).is_err());
    }
}
fn coding() -> CodingRecord<'static> {
    CodingRecord::new(CodingId::new(19), 1, &[])
}

fn payload(
    surface: SurfaceDescriptor,
    codings: &[CodingRecord<'_>],
    groups: Option<&[UnitGroupRecord]>,
    indexes: Option<&[u8]>,
    data: &[u8],
    palette: Option<&[u8]>,
    integrity: Option<&[u32]>,
) -> Vec<u8> {
    let mut surface_bytes = vec![0; SURFACE_RECORD_LEN];
    surface.encode_record_into(&mut surface_bytes).unwrap();
    let mut coding_bytes = vec![0; CodingTable::encoded_len(codings).unwrap()];
    CodingTable::encode_into(codings, &mut coding_bytes).unwrap();
    let mut sections = vec![
        (MediaSectionKind::SURFACE, surface_bytes),
        (MediaSectionKind::CODINGS, coding_bytes),
    ];
    if let Some(groups) = groups {
        let mut bytes = vec![0; groups.len() * UNIT_GROUP_RECORD_LEN];
        for (index, group) in groups.iter().enumerate() {
            group
                .encode_into(&mut bytes[index * UNIT_GROUP_RECORD_LEN..])
                .unwrap();
        }
        sections.push((MediaSectionKind::UNIT_GROUPS, bytes));
    }
    if let Some(indexes) = indexes {
        sections.push((MediaSectionKind::UNIT_INDEX, indexes.to_vec()));
    }
    if let Some(palette) = palette {
        sections.push((MediaSectionKind::COLOR_TABLE, palette.to_vec()));
    }
    if let Some(lengths) = integrity {
        sections.push((MediaSectionKind::INTEGRITY, vec![0; lengths.len() * 12]));
    }
    sections.push((MediaSectionKind::DATA, data.to_vec()));
    let header_end = MEDIA_HEADER_LEN + sections.len() * MEDIA_SECTION_LEN;
    let data_offset = header_end
        + sections[..sections.len() - 1]
            .iter()
            .map(|(_, bytes)| bytes.len())
            .sum::<usize>();
    if let Some(lengths) = integrity {
        let integrity_index = sections.len() - 2;
        let records = &mut sections[integrity_index].1;
        let mut offset = data_offset as u32;
        for (index, &length) in lengths.iter().enumerate() {
            write_u32_le(records, index * 12, offset);
            write_u32_le(records, index * 12 + 4, length);
            offset += length;
        }
    }
    let mut bytes = vec![0; header_end];
    bytes[0] = MEDIA_VERSION;
    bytes[1] = u8::from(integrity.is_some());
    write_u16_le(&mut bytes, 2, sections.len() as u16);
    for (index, (kind, body)) in sections.iter().enumerate() {
        let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
        write_u16_le(&mut bytes, entry, kind.raw());
        write_u16_le(&mut bytes, entry + 2, MediaSectionFlags::REQUIRED.bits());
        let offset = bytes.len() as u32;
        write_u32_le(&mut bytes, entry + 4, offset);
        write_u32_le(&mut bytes, entry + 8, body.len() as u32);
        bytes.extend_from_slice(body);
    }
    if integrity.is_none() {
        bytes.extend_from_slice(&[0; 4]);
    }
    crate::image::test_support::refresh_crc(&mut bytes);
    bytes
}

fn split_records() -> [UnitGroupRecord; 2] {
    [
        UnitGroupRecord::new(0, 0..2)
            .unwrap()
            .with_tiles(1, 1)
            .with_selection(GroupSelection::List(2)),
        UnitGroupRecord::new(0, 2..4)
            .unwrap()
            .with_tiles(1, 1)
            .with_selection(GroupSelection::List(2))
            .with_index_offset(8),
    ]
}
const SPLIT_INDEX: [u8; 16] = [0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0];

#[test]
fn constant_space_and_prepared_validation_share_static_contracts() {
    let mut overlap = SPLIT_INDEX;
    overlap[8] = 1;
    let mut temporal = split_records();
    temporal[1] = temporal[1].with_reference(ReferenceMode::Previous);
    let mut missing_coding = split_records();
    missing_coding[1] = UnitGroupRecord::new(3, 2..4).unwrap();
    for (records, index, data) in [
        (split_records(), SPLIT_INDEX, &[1; 4][..]),
        (split_records(), overlap, &[1; 4][..]),
        (temporal, SPLIT_INDEX, &[1; 4][..]),
        (missing_coding, SPLIT_INDEX, &[1; 4][..]),
        (split_records(), SPLIT_INDEX, &[1; 5][..]),
    ] {
        let bytes = payload(
            surface(),
            &[coding()],
            Some(&records),
            Some(&index),
            data,
            None,
            None,
        );
        let image = EncodedImageView::open(&bytes).unwrap();
        let mut workspace = [None; 2];
        let prepared = image
            .groups_into(&mut workspace, &mut CoverageBudget::new(10_000))
            .map(|_| ());
        assert_eq!(
            image.validate_groups(&mut CoverageBudget::new(10_000)),
            prepared
        );
    }
    let yuv = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let records = [
        UnitGroupRecord::new(0, 0..9)
            .unwrap()
            .with_planes(super::super::GroupPlanes::Plane(0)),
        UnitGroupRecord::new(0, 9..17)
            .unwrap()
            .with_planes(super::super::GroupPlanes::Plane(1)),
    ];
    let bytes = payload(
        yuv,
        &[coding()],
        Some(&records),
        None,
        &[128; 17],
        None,
        None,
    );
    EncodedImageView::open(&bytes)
        .unwrap()
        .validate_groups(&mut CoverageBudget::new(10_000))
        .unwrap();
}

#[test]
fn constant_space_validation_charges_repeated_parsing_before_work() {
    let bytes = payload(
        surface(),
        &[coding()],
        Some(&split_records()),
        Some(&SPLIT_INDEX),
        &[1; 4],
        None,
        None,
    );
    let image = EncodedImageView::open(&bytes).unwrap();
    let mut budget = CoverageBudget::new(10_000);
    image.validate_groups(&mut budget).unwrap();
    let used = 10_000 - budget.remaining();
    let mut workspace = [None; 2];
    let mut cached_budget = CoverageBudget::new(10_000);
    image
        .groups_into(&mut workspace, &mut cached_budget)
        .unwrap();
    assert!(used > 10_000 - cached_budget.remaining() + 2 * (1 + 2 + 16));
    assert_eq!(
        image.validate_groups(&mut CoverageBudget::new(used)),
        Ok(())
    );
    assert_eq!(
        image.validate_groups(&mut CoverageBudget::new(used - 1)),
        Err(EncodedImageError::Coverage(CoverageError::BudgetExceeded))
    );
    let mut too_small = CoverageBudget::new(18);
    assert_eq!(
        image.validate_groups(&mut too_small),
        Err(EncodedImageError::Coverage(CoverageError::BudgetExceeded))
    );
    assert_eq!(too_small.remaining(), 18);
    let empty =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let bytes = payload(empty, &[coding()], None, None, &[], None, None);
    EncodedImageView::open(&bytes)
        .unwrap()
        .validate_groups(&mut CoverageBudget::new(100))
        .unwrap();
}

#[test]
fn group_validation_does_not_claim_checksum_or_codec_support() {
    let mut bytes = payload(
        surface(),
        &[coding()],
        Some(&split_records()),
        Some(&SPLIT_INDEX),
        &[1; 4],
        None,
        Some(&[2, 2]),
    );
    let data = crate::image::test_support::data_offset(&bytes);
    bytes[data] ^= 1;
    let image = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(
        image.validate_groups(&mut CoverageBudget::new(10_000)),
        Ok(())
    );
    assert!(image.validate_data().is_err());
    let mut workspace = [None; 2];
    let groups = image
        .groups_into(&mut workspace, &mut CoverageBudget::new(100))
        .unwrap();
    assert!(matches!(
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(super::super::SurfaceRequirements::new()),
        Err(super::super::UnitDecodeError::UnsupportedCoding(_))
    ));
}

#[test]
fn implicit_group_metadata_and_data_validation_remain_separate() {
    let bytes = payload(
        surface(),
        &[coding()],
        None,
        None,
        &[1, 2, 3, 4],
        None,
        None,
    );
    let image = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(image.group_count(), 1);
    let mut workspace = [None; 2];
    let groups = image
        .groups_into(&mut workspace, &mut CoverageBudget::new(10))
        .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups.get(0).unwrap().get(0).unwrap().data(), &[1, 2, 3, 4]);
    assert_eq!(groups.validate_unit(0, 0), Ok(4));
    assert_eq!(
        groups.validate_unit(1, 0),
        Err(EncodedImageError::GroupOutOfBounds(1))
    );
    assert_eq!(
        groups.validate_unit(0, 1),
        Err(EncodedImageError::UnitOutOfBounds {
            group: 0,
            ordinal: 1
        })
    );
    assert_eq!(groups.iter().next(), groups.get(0));
    assert_eq!(groups.iter().last(), groups.get(0));
    assert!(workspace[1].is_none());
    let mut corrupt = bytes.clone();
    let offset = crate::image::test_support::data_offset(&corrupt);
    corrupt[offset] ^= 1;
    let image = EncodedImageView::open(&corrupt).unwrap();
    let mut workspace = [None];
    assert!(
        image
            .groups_into(&mut workspace, &mut CoverageBudget::new(10))
            .is_ok()
    );
    assert!(image.validate_data().is_err());
    let empty_surface =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let empty = payload(empty_surface, &[coding()], None, None, &[], None, None);
    let mut workspace = [None];
    assert!(
        EncodedImageView::open(&empty)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(10))
            .unwrap()
            .get(0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn prepared_sparse_groups_preserve_local_checksum_boundaries() {
    let mut bytes = payload(
        surface(),
        &[coding()],
        Some(&split_records()),
        Some(&SPLIT_INDEX),
        &[1, 2, 3, 4],
        None,
        Some(&[2, 2]),
    );
    let data_offset = crate::image::test_support::data_offset(&bytes);
    bytes[data_offset + 3] ^= 1;
    let image = EncodedImageView::open(&bytes).unwrap();
    let mut workspace = [None; 3];
    let groups = image
        .groups_into(&mut workspace, &mut CoverageBudget::new(100))
        .unwrap();
    assert_eq!(
        groups.get(1).unwrap().cell(3).unwrap().data().as_ptr(),
        bytes[data_offset + 3..].as_ptr()
    );
    assert_eq!(groups.validate_unit(0, 0), Ok(2));
    assert!(matches!(
        groups.validate_unit(1, 1),
        Err(EncodedImageError::Media(
            MediaPayloadError::RangeCrcMismatch { .. }
        ))
    ));
    assert!(groups.image().validate_data().is_err());
    assert_eq!(groups.iter().nth_back(1), groups.get(0));
    assert!(workspace[2].is_none());
}

#[test]
fn ambiguous_and_noncanonical_sections_do_not_create_default_groups() {
    let ambiguous = payload(
        surface(),
        &[coding(), coding()],
        None,
        None,
        &[1],
        None,
        None,
    );
    assert_eq!(
        EncodedImageView::open(&ambiguous),
        Err(EncodedImageError::AmbiguousImplicitGroup)
    );
    let empty_groups = payload(surface(), &[coding()], Some(&[]), None, &[1], None, None);
    assert_eq!(
        EncodedImageView::open(&empty_groups),
        Err(EncodedImageError::InvalidGroupTableLength(0))
    );
    let orphan = payload(surface(), &[coding()], None, Some(&[0]), &[1], None, None);
    assert_eq!(
        EncodedImageView::open(&orphan),
        Err(EncodedImageError::AmbiguousImplicitGroup)
    );
    let empty_index = payload(
        surface(),
        &[coding()],
        Some(&[UnitGroupRecord::new(0, 0..1).unwrap()]),
        Some(&[]),
        &[1],
        None,
        None,
    );
    assert_eq!(
        EncodedImageView::open(&empty_index),
        Err(EncodedImageError::EmptyIndexSection)
    );
    let base = payload(surface(), &[coding()], None, None, &[1], None, None);
    for (field, kind, expected) in [
        (
            MEDIA_HEADER_LEN + MEDIA_SECTION_LEN,
            MediaSectionKind::SURFACE,
            EncodedImageError::DuplicateSection(MediaSectionKind::SURFACE),
        ),
        (
            MEDIA_HEADER_LEN + MEDIA_SECTION_LEN,
            MediaSectionKind::PLANES,
            EncodedImageError::UnexpectedSection(MediaSectionKind::PLANES),
        ),
        (
            MEDIA_HEADER_LEN + MEDIA_SECTION_LEN,
            MediaSectionKind::new(0x9999).unwrap(),
            EncodedImageError::UnknownRequiredSection(MediaSectionKind::new(0x9999).unwrap()),
        ),
    ] {
        let mut changed = base.clone();
        write_u16_le(&mut changed, field, kind.raw());
        crate::image::test_support::refresh_crc(&mut changed);
        assert_eq!(EncodedImageView::open(&changed), Err(expected));
    }
    let mut optional = base.clone();
    write_u16_le(&mut optional, MEDIA_HEADER_LEN + 2, 0);
    crate::image::test_support::refresh_crc(&mut optional);
    assert_eq!(
        EncodedImageView::open(&optional),
        Err(EncodedImageError::SectionMustBeRequired(
            MediaSectionKind::SURFACE
        ))
    );
}

#[test]
fn static_coverage_order_and_reference_checks_precede_success() {
    let mut wrong_index = SPLIT_INDEX;
    wrong_index[8] = 1;
    let overlap = payload(
        surface(),
        &[coding()],
        Some(&split_records()),
        Some(&wrong_index),
        &[1; 4],
        None,
        None,
    );
    let mut workspace = [None; 2];
    assert!(matches!(
        EncodedImageView::open(&overlap)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::Coverage(CoverageError::Overlap { .. }))
    ));
    let mut records = split_records();
    records[1] = records[1].with_reference(ReferenceMode::Previous);
    let reference = payload(
        surface(),
        &[coding()],
        Some(&records),
        Some(&SPLIT_INDEX),
        &[1; 4],
        None,
        None,
    );
    assert!(matches!(
        EncodedImageView::open(&reference)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::ReferenceInStaticImage(1))
    ));
    records = split_records();
    records[1] = UnitGroupRecord::new(0, 1..3)
        .unwrap()
        .with_tiles(1, 1)
        .with_selection(GroupSelection::List(2))
        .with_index_offset(8);
    let overlap_data = payload(
        surface(),
        &[coding()],
        Some(&records),
        Some(&SPLIT_INDEX),
        &[1; 4],
        None,
        None,
    );
    assert!(matches!(
        EncodedImageView::open(&overlap_data)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::NonCanonicalDataRange { index: 1, .. })
    ));
    let suffix = payload(
        surface(),
        &[coding()],
        Some(&split_records()),
        Some(&SPLIT_INDEX),
        &[1; 5],
        None,
        None,
    );
    assert!(matches!(
        EncodedImageView::open(&suffix)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::UnreferencedData)
    ));
}

#[test]
fn palette_alignment_and_workspace_contracts_are_explicit() {
    let indexed = SurfaceDescriptor::new(4, 1, SampleLayout::I1, ColorDescription::SRGB).unwrap();
    let colors = [10, 20, 30, 255, 40, 50, 60, 255];
    let bytes = payload(indexed, &[coding()], None, None, &[1], Some(&colors), None);
    assert_eq!(
        EncodedImageView::open(&bytes)
            .unwrap()
            .color_table()
            .unwrap()
            .as_bytes(),
        colors
    );
    let absent = payload(indexed, &[coding()], None, None, &[1], None, None);
    assert_eq!(
        EncodedImageView::open(&absent),
        Err(EncodedImageError::MissingColorTable)
    );
    let extra = payload(
        surface(),
        &[coding()],
        None,
        None,
        &[1],
        Some(&colors),
        None,
    );
    assert_eq!(
        EncodedImageView::open(&extra),
        Err(EncodedImageError::UnexpectedColorTable)
    );
    let aligned = payload(
        surface(),
        &[coding()],
        Some(&[UnitGroupRecord::new(0, 0..4)
            .unwrap()
            .with_input_alignment(64)]),
        None,
        &[1; 4],
        None,
        None,
    );
    let offset = crate::image::test_support::data_offset(&aligned) as u32;
    let origin = (64 - offset % 64) % 64;
    let mut workspace = [None];
    assert!(
        EncodedImageView::open_at(&aligned, origin)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(10))
            .is_ok()
    );
    assert!(matches!(
        EncodedImageView::open_at(&aligned, origin + 1)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(10)),
        Err(EncodedImageError::FileAddressUnaligned { .. })
    ));
    let mut empty = [];
    let mut budget = CoverageBudget::new(10);
    assert!(matches!(
        EncodedImageView::open(&aligned)
            .unwrap()
            .groups_into(&mut empty, &mut budget),
        Err(EncodedImageError::WorkspaceTooSmall {
            needed: 1,
            available: 0
        })
    ));
    assert_eq!(budget.remaining(), 10);
    assert!(matches!(
        EncodedImageView::open(&aligned)
            .unwrap()
            .groups_into(&mut workspace, &mut CoverageBudget::new(0)),
        Err(EncodedImageError::Coverage(CoverageError::BudgetExceeded))
    ));
}
