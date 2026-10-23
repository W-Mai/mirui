use super::*;
use crate::{
    coding::Rle,
    image::{
        ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageView, SampleLayout,
        SurfaceRequirements, UnitGroup,
    },
    media::{MediaPayload, MediaSectionFlags},
    types::DataIntegrity,
    wire::{write_u16_le, write_u32_le},
};
use alloc::vec::Vec;

fn scan(payload: &[u8], width: usize, offset: u32) -> Result<EncodedImageScan, EncodedImageError> {
    let layout =
        MediaLayout::parse(&payload[..8], payload.len()).map_err(EncodedImageError::Media)?;
    let mut scan = EncodedImageScan::new(layout, offset);
    assert!(scan.surface().is_err());
    assert!(scan.section(MediaSectionKind::SURFACE).is_none());
    while let Some(range) = scan.read_range() {
        let before = scan.read_range();
        assert!(scan.accept(&[]).is_err());
        assert_eq!(before, scan.read_range());
        scan.accept(&payload[range.start..range.start + width.min(range.len())])?;
    }
    Ok(scan)
}

fn bind<'a>(
    scan: &EncodedImageScan,
    payload: &'a [u8],
) -> Result<EncodedImageLayout<'a>, EncodedImageError> {
    let bytes = |kind| {
        scan.section(kind)
            .map(|s| &payload[s.offset() as usize..(s.offset() + s.size()) as usize])
    };
    scan.finish(
        bytes(MediaSectionKind::CODINGS).unwrap(),
        bytes(MediaSectionKind::UNIT_GROUPS),
        bytes(MediaSectionKind::UNIT_INDEX),
    )
}

#[test]
fn bounded_directory_reads_match_resident_encoded_images_and_original_decoder() {
    for sample in [SampleLayout::A8, SampleLayout::I4] {
        let color = if sample == SampleLayout::A8 {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(4, 2, sample, color).unwrap();
        let input = if sample == SampleLayout::A8 {
            &[0x83, 1, 0x83, 2][..]
        } else {
            &[0x81, 0x11, 0x81, 0x22][..]
        };
        let coding = Rle::new();
        let group = UnitGroup::builder(surface, coding.record(), input)
            .with_tiles(2, 2)
            .build()
            .unwrap();
        let groups = [group];
        let colors = [0; 64];
        for indexed in [false, true] {
            let mut asset = EncodedImageAsset::from_groups(surface, &groups);
            if sample == SampleLayout::I4 {
                asset = asset.with_color_table(&colors);
            }
            if indexed {
                asset = asset.with_integrity(DataIntegrity::Indexed(&[2, 4]));
            }
            let payload = asset.encode().unwrap();
            let expected = EncodedImageView::open_at(&payload, 64).unwrap();
            for width in [1, 7, 256] {
                let scan = scan(&payload, width, 64).unwrap();
                assert_eq!(scan.surface().unwrap(), expected.surface());
                for section in expected
                    .media()
                    .sections()
                    .filter(|s| s.descriptor().kind() != MediaSectionKind::INTEGRITY)
                {
                    assert_eq!(
                        scan.section(section.descriptor().kind()),
                        Some(section.descriptor())
                    );
                }
                let actual = bind(&scan, &payload).unwrap();
                let mut a = CoverageBudget::new(10000);
                let mut b = CoverageBudget::new(10000);
                assert_eq!(
                    actual.validate_groups(&mut a),
                    expected.validate_groups(&mut b)
                );
                assert_eq!(a.remaining(), b.remaining());
                let data = scan.section(MediaSectionKind::DATA).unwrap().offset() as usize;
                let group = actual.group(0).unwrap();
                for ordinal in 0..group.len() {
                    let range = group.unit_range(ordinal).unwrap();
                    let unit = group
                        .unit(
                            ordinal,
                            &payload[data + range.start as usize..data + range.end as usize],
                        )
                        .unwrap();
                    let plan = unit.decode_plan(SurfaceRequirements::new()).unwrap();
                    let mut pixels = [0; 4];
                    let decoded = plan.decode_into(&mut pixels).unwrap();
                    assert!(decoded.plane(0).unwrap().bytes().iter().all(|v| *v
                        == if sample == SampleLayout::A8 {
                            ordinal as u8 + 1
                        } else {
                            (ordinal as u8 + 1) * 17
                        }));
                }
            }
        }
    }
}

#[test]
fn window_and_resident_paths_reject_the_same_section_contract_violations() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 7])
        .encode()
        .unwrap();
    let descriptors: Vec<_> = MediaPayload::open(&payload)
        .unwrap()
        .sections()
        .enumerate()
        .map(|(index, section)| (8 + 12 * index, section.descriptor()))
        .collect();
    let surface_record = descriptors
        .iter()
        .find(|(_, d)| d.kind() == MediaSectionKind::SURFACE)
        .unwrap()
        .0;
    let coding_record = descriptors
        .iter()
        .find(|(_, d)| d.kind() == MediaSectionKind::CODINGS)
        .unwrap()
        .0;
    let data_record = descriptors
        .iter()
        .find(|(_, d)| d.kind() == MediaSectionKind::DATA)
        .unwrap()
        .0;
    for case in 0..6 {
        let mut bytes = payload.clone();
        match case {
            0 => write_u16_le(
                &mut bytes,
                coding_record + 2,
                MediaSectionFlags::NONE.bits(),
            ),
            1 => write_u16_le(&mut bytes, coding_record, MediaSectionKind::SURFACE.raw()),
            2 => write_u16_le(&mut bytes, coding_record, MediaSectionKind::PLANES.raw()),
            3 => write_u16_le(&mut bytes, coding_record, 0xffff),
            4 => write_u32_le(
                &mut bytes,
                surface_record + 8,
                SURFACE_RECORD_LEN as u32 - 1,
            ),
            5 => {
                write_u16_le(&mut bytes, data_record, 0xffff);
                write_u16_le(&mut bytes, data_record + 2, 0);
            }
            _ => unreachable!(),
        }
        crate::media::refresh_checksums(&mut bytes);
        let expected = EncodedImageView::open(&bytes).unwrap_err();
        for width in [1, 32] {
            assert_eq!(scan(&bytes, width, 0).err(), Some(expected));
        }
    }
}

#[test]
fn metadata_bodies_must_match_declared_presence_and_lengths() {
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x81, 7])
        .encode()
        .unwrap();
    let scanned = scan(&payload, 3, 0).unwrap();
    let descriptor = scanned.section(MediaSectionKind::CODINGS).unwrap();
    let codings =
        &payload[descriptor.offset() as usize..(descriptor.offset() + descriptor.size()) as usize];
    assert!(
        scanned
            .finish(&codings[..codings.len() - 1], None, None)
            .is_err()
    );
    assert!(scanned.finish(codings, Some(&[]), None).is_err());
    assert!(scanned.finish(codings, None, Some(&[])).is_err());
    let overflow = scan(&payload, 3, u32::MAX).unwrap();
    assert!(matches!(
        overflow.finish(codings, None, None),
        Err(EncodedImageError::SizeOverflow)
    ));
    assert_eq!(bind(&scanned, &payload).unwrap().group_count(), 1);
}
