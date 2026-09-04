use super::*;
use crate::{
    PayloadLimits,
    coding::Rle,
    image::{ColorDescription, SampleLayout, SurfaceRequirements},
    media::{MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION},
    wire::{write_u16_le, write_u32_le},
};

fn payload(extra: usize, indexed: bool) -> alloc::vec::Vec<u8> {
    payload_with_stream(&[0x83, 7], extra, indexed)
}

fn payload_with_stream(stream: &[u8], extra: usize, indexed: bool) -> alloc::vec::Vec<u8> {
    let mut coding = [0; 12];
    CodingTable::encode_into(&[Rle::new().record()], &mut coding).unwrap();
    let mut sections = alloc::vec![
        (MediaSectionKind::CODINGS, coding.to_vec()),
        (MediaSectionKind::DATA, stream.to_vec()),
        (MediaSectionKind::DATA, alloc::vec![42; extra]),
    ];
    if indexed {
        sections.push((
            MediaSectionKind::INTEGRITY,
            alloc::vec![0; (usize::from(!stream.is_empty()) + usize::from(extra != 0)) * 12],
        ));
    }
    let mut bytes = alloc::vec![0; MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * sections.len()];
    bytes[0] = MEDIA_VERSION;
    bytes[1] = u8::from(indexed);
    write_u16_le(&mut bytes, 2, sections.len() as u16);
    let mut ranges = [(0, 0); 2];
    for (index, (kind, data)) in sections.iter().enumerate() {
        let offset = bytes.len() as u32;
        let entry = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * index;
        write_u16_le(&mut bytes, entry, kind.raw());
        write_u16_le(&mut bytes, entry + 2, 1);
        write_u32_le(&mut bytes, entry + 4, offset);
        write_u32_le(&mut bytes, entry + 8, data.len() as u32);
        bytes.extend_from_slice(data);
        if *kind == MediaSectionKind::DATA {
            ranges[index - 1] = (offset, data.len() as u32);
        }
        if *kind == MediaSectionKind::INTEGRITY {
            for (ordinal, (start, size)) in ranges
                .into_iter()
                .filter(|(_, size)| *size != 0)
                .enumerate()
            {
                write_u32_le(&mut bytes, offset as usize + ordinal * 12, start);
                write_u32_le(&mut bytes, offset as usize + ordinal * 12 + 4, size);
            }
        }
    }
    if !indexed {
        bytes.extend_from_slice(&[0; 4]);
    }
    crate::media::refresh_checksums(&mut bytes);
    bytes
}

fn bind(bytes: &[u8]) -> EncodedImageView<'_> {
    let media = MediaPayload::open(bytes).unwrap();
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    EncodedImageView::from_sections(
        media,
        surface,
        EncodedSections {
            codings: media.get(0),
            data: media.get(1),
            records: None,
            indexes: None,
            color_table: None,
        },
        None,
    )
    .unwrap()
}

fn required_preflight_work(image: EncodedImageView<'_>) -> u64 {
    let (mut low, mut high) = (0, 100_000);
    while low < high {
        let mid = low + (high - low) / 2;
        if image
            .preflight(&PayloadLimits::HOST.with_max_image_work(mid))
            .is_ok()
        {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    low
}

#[test]
fn whole_decode_and_preflight_charge_every_data_body_they_verify() {
    let base = payload(0, false);
    let base_work = required_preflight_work(bind(&base));
    for indexed in [false, true] {
        let bytes = payload(100, indexed);
        let image = bind(&bytes);
        assert_eq!(image.checksum_bytes, 102);
        assert_eq!(required_preflight_work(image), base_work + 100);
        assert!(
            image
                .preflight(&PayloadLimits::HOST.with_max_image_work(base_work))
                .is_err()
        );
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        let plan = groups
            .decode_plan(SurfaceRequirements::new(), &PayloadLimits::HOST)
            .unwrap();
        assert_eq!(plan.input_byte_len(), 2);
        assert_eq!(plan.checksum_byte_len(), 102);
        assert!(
            groups
                .decode_plan(
                    SurfaceRequirements::new(),
                    &PayloadLimits::HOST.with_max_image_work(plan.work() - 1)
                )
                .is_err()
        );
        groups
            .decode_plan(
                SurfaceRequirements::new(),
                &PayloadLimits::HOST.with_max_image_work(plan.work()),
            )
            .unwrap();
        let mut output = [0; 4];
        let mut workspace = [0; 4];
        assert_eq!(
            plan.decode_into(&mut output, &mut workspace)
                .unwrap()
                .plane(0)
                .unwrap()
                .bytes(),
            &[7; 4]
        );
        // Shared binding is not a second public IMAGE directory policy.
        assert!(matches!(
            EncodedImageView::open(&bytes),
            Err(EncodedImageError::DuplicateSection(MediaSectionKind::DATA))
        ));
    }
}

#[test]
fn empty_selected_surface_still_accounts_for_whole_media_validation() {
    for indexed in [false, true] {
        let bytes = payload_with_stream(&[], 31, indexed);
        let media = MediaPayload::open(&bytes).unwrap();
        let surface =
            SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let image = EncodedImageView::from_sections(
            media,
            surface,
            EncodedSections {
                codings: media.get(0),
                data: media.get(1),
                records: None,
                indexes: None,
                color_table: None,
            },
            None,
        )
        .unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        let plan = groups
            .decode_plan(SurfaceRequirements::new(), &PayloadLimits::HOST)
            .unwrap();
        assert_eq!(plan.unit_count(), 0);
        assert_eq!(plan.input_byte_len(), 0);
        assert_eq!(plan.checksum_byte_len(), 31);
        assert_eq!(plan.workspace_requirements().byte_len(), 0);
        let region = surface.region(0, 0, 0, 0).unwrap();
        let crop = groups
            .decode_region_plan(region, SurfaceRequirements::new(), &PayloadLimits::HOST)
            .unwrap();
        assert_eq!(crop.checksum_byte_len(), 0);
    }
}

#[test]
fn region_integrity_is_partition_scoped_while_whole_integrity_covers_all_data() {
    for indexed in [false, true] {
        let mut bytes = payload(100, indexed);
        let other = MediaPayload::open(&bytes)
            .unwrap()
            .get(2)
            .unwrap()
            .descriptor()
            .offset() as usize;
        bytes[other] ^= 1;
        let image = bind(&bytes);
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert!(matches!(
            groups.decode_plan(SurfaceRequirements::new(), &PayloadLimits::HOST),
            Err(DecodeError::Image(EncodedImageError::Media(_)))
        ));
        assert!(matches!(
            image.preflight(&PayloadLimits::HOST),
            Err(EncodedImageError::Media(_))
        ));
        let region = image.surface().region(1, 0, 2, 1).unwrap();
        let plan =
            groups.decode_region_plan(region, SurfaceRequirements::new(), &PayloadLimits::HOST);
        if indexed {
            let plan = plan.unwrap();
            assert_eq!(plan.checksum_byte_len(), 2);
            assert_eq!(plan.input_byte_len(), 2);
            let mut output = [0; 2];
            let mut workspace = [0; 4];
            assert_eq!(
                plan.decode_into(&mut output, &mut workspace)
                    .unwrap()
                    .plane(0)
                    .unwrap()
                    .bytes(),
                &[7; 2]
            );
        } else {
            assert!(matches!(
                plan,
                Err(DecodeError::Image(EncodedImageError::Media(_)))
            ));
        }
    }
}
