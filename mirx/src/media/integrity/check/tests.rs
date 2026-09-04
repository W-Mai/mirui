use super::*;
use crate::{
    coding::Rle,
    image::{ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor},
    media::DataIntegrity,
};

#[test]
fn planned_cost_matches_exact_intersecting_partitions_without_reading_data() {
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let ends = [3, 5, 8];
    for integrity in [DataIntegrity::Whole, DataIntegrity::Indexed(&ends)] {
        let mut bytes = EncodedImageAsset::new(
            surface,
            Rle::new().record(),
            &[0x81, 1, 0x81, 2, 0x81, 3, 0x81, 4],
        )
        .with_integrity(integrity)
        .encode()
        .unwrap();
        let media = MediaPayload::open(&bytes).unwrap();
        let start = media
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset();
        for first in 0..=8 {
            for end in first..=8 {
                let expected = if first == end {
                    0
                } else if integrity == DataIntegrity::Whole {
                    8
                } else {
                    [(0, 3), (3, 5), (5, 8)]
                        .into_iter()
                        .filter(|&(a, b)| a < end && first < b)
                        .map(|(a, b)| b - a)
                        .sum()
                };
                let plan = media.data_check_plan(start + first..start + end).unwrap();
                assert_eq!(plan.byte_len(), expected);
                plan.verify().unwrap();
                assert_eq!(
                    media.validate_data_range(start + first..start + end),
                    Ok(expected)
                );
            }
        }
        for range in [start - 1..start + 1, start..start + 9, start + 2..start + 1] {
            assert!(matches!(
                media.data_check_plan(range),
                Err(MediaPayloadError::InvalidDataRange)
            ));
        }
        bytes[start as usize + 7] ^= 1;
        let media = MediaPayload::open(&bytes).unwrap();
        let plan = media.data_check_plan(start + 6..start + 7).unwrap();
        assert_eq!(
            plan.byte_len(),
            if integrity == DataIntegrity::Whole {
                8
            } else {
                3
            }
        );
        assert!(plan.verify().is_err());
        let unaffected = media.data_check_plan(start..start + 1).unwrap();
        assert_eq!(
            unaffected.verify().is_ok(),
            integrity != DataIntegrity::Whole
        );
        media
            .data_check_plan(start + 8..start + 8)
            .unwrap()
            .verify()
            .unwrap();
    }
}

#[test]
fn empty_data_requests_do_not_verify_an_unrequested_checksum() {
    let surface =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    for integrity in [DataIntegrity::Whole, DataIntegrity::Indexed(&[])] {
        let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[])
            .with_integrity(integrity)
            .encode()
            .unwrap();
        let media = MediaPayload::open(&bytes).unwrap();
        let start = media
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset();
        let plan = media.data_check_plan(start..start).unwrap();
        assert_eq!(plan.byte_len(), 0);
        plan.verify().unwrap();
    }
}
