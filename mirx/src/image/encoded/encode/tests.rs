use super::*;
mod grouped;
mod integrity;
use crate::{
    coding::{Lz4, Pixel, Rle},
    image::{
        ColorDescription, CoverageBudget, EncodedImageError, EncodedImageView, SampleLayout,
        SurfaceRequirements, UnitDecodeError,
    },
};

#[test]
fn retained_coding_tables_emit_identical_native_bytes_without_record_arrays() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let data = [0x83, 7];
    let codings = [Rle::new().record(), CodingRecord::RAW];
    let records = [UnitGroupRecord::new(0, 0..2).unwrap()];
    let native = EncodedImageAsset::from_groups(surface, &codings, &records, &data);
    let bytes = native.encode().unwrap();
    let image = EncodedImageView::open(&bytes).unwrap();
    let retained =
        EncodedImageAsset::from_codings(surface, image.codings(), &data).with_groups(&records);
    assert_eq!(retained.encode().unwrap(), bytes);
    assert!(retained.matches_payload(&bytes).unwrap());
    retained.preflight(&crate::PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        retained.codings().rev().collect::<Vec<_>>(),
        [CodingRecord::RAW, Rle::new().record()]
    );
    assert!(matches!(
        EncodedImageAsset::from_codings(surface, image.codings(), &data).encoded_len(),
        Err(ImageEncodeError::Preflight(
            EncodedImageError::AmbiguousImplicitGroup
        ))
    ));

    let params = [3, 5, 8];
    let native = EncodedImageAsset::new(
        surface,
        CodingRecord::new(CodingId::new(500), 2, &params),
        &data,
    );
    let bytes = native.encode().unwrap();
    let image = EncodedImageView::open(&bytes).unwrap();
    let retained = EncodedImageAsset::from_codings(surface, image.codings(), &data);
    assert_eq!(retained.encode().unwrap(), bytes);
    assert_eq!(
        retained.codings().next().unwrap().params().as_ptr(),
        image.codings().get(0).unwrap().params().as_ptr()
    );
    assert_eq!(
        retained.preflight(&crate::PayloadLimits::EMBEDDED),
        native.preflight(&crate::PayloadLimits::EMBEDDED)
    );
}

#[test]
fn asset_preflight_admits_exactly_supported_syntax_without_serializing() {
    use crate::{
        PayloadLimits,
        image::{CoverageError, ImageEncodeError},
    };
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
    assert_eq!(asset.preflight(&PayloadLimits::EMBEDDED), Ok(()));
    let malformed = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff]);
    assert!(matches!(
        malformed.preflight(&PayloadLimits::EMBEDDED),
        Err(ImageEncodeError::Preflight(EncodedImageError::Unit {
            group: 0,
            ordinal: 0,
            ..
        }))
    ));
    let unknown =
        EncodedImageAsset::new(surface, CodingRecord::new(CodingId::new(511), 1, &[]), &[1]);
    assert!(matches!(
        unknown.preflight(&PayloadLimits::EMBEDDED),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coding {
            group: 0,
            ..
        }))
    ));
    for limits in [
        PayloadLimits::EMBEDDED.with_max_decoded_bytes(7),
        PayloadLimits::EMBEDDED.with_max_raster_groups(0),
        PayloadLimits::EMBEDDED.with_max_raster_units(0),
        PayloadLimits::EMBEDDED.with_max_raster_work(0),
    ] {
        assert!(asset.preflight(&limits).is_err());
    }
    // Small decoded output cannot authorize a two-gigabyte padding allocation.
    let padded = asset.with_input_alignment(1 << 31);
    assert!(padded.encoded_len().unwrap() > 1 << 31);
    assert_eq!(
        padded.preflight(&PayloadLimits::EMBEDDED),
        Err(ImageEncodeError::Preflight(EncodedImageError::Coverage(
            CoverageError::BudgetExceeded
        )))
    );
}

#[test]
fn asset_preflight_charges_reader_work_and_canonical_output_without_double_profiles() {
    use crate::PayloadLimits;
    for layout in [
        SampleLayout::A8,
        SampleLayout::RGB565_A8,
        SampleLayout::NV12,
        SampleLayout::I420,
    ] {
        let color = if layout.is_alpha() {
            ColorDescription::NONE
        } else if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else {
            ColorDescription::SRGB
        };
        for width in [0, 4] {
            let surface = SurfaceDescriptor::new(width, 2, layout, color).unwrap();
            let size: usize = surface
                .planes()
                .map(|p| (p.minimum_stride().unwrap() * p.height()) as usize)
                .sum();
            let mut stream = [0; 128];
            let len = Rle::new()
                .encode_into(&[42; 64][..size], &mut stream)
                .unwrap();
            for alignment in [1, 64] {
                let asset = EncodedImageAsset::new(surface, Rle::new().record(), &stream[..len])
                    .with_input_alignment(alignment);
                let payload = asset.encode().unwrap();
                let image = EncodedImageView::open(&payload).unwrap();
                let minimum = (0..1024)
                    .find(|work| {
                        image
                            .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(*work))
                            .is_ok()
                    })
                    .unwrap();
                let combined = minimum + payload.len() as u64;
                assert!(
                    asset
                        .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(combined - 1))
                        .is_err()
                );
                assert_eq!(
                    asset.preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(combined)),
                    Ok(())
                );
            }
        }
    }
}

#[test]
fn single_stream_omission_round_trips_each_scalar_profile() {
    let surface =
        SurfaceDescriptor::new(3, 2, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let samples = [
        17, 42, 91, 17, 42, 91, 0, 1, 2, 0, 1, 2, 55, 56, 57, 55, 56, 57,
    ];
    let pixel = Pixel::new(surface.sample_layout()).unwrap();
    let rle = Rle::new().with_element_size(3).unwrap();
    let lz4 = Lz4::new();
    let mut table = [0; Lz4::TABLE_LEN];
    for coding in [pixel.record(), rle.record(), lz4.record()] {
        let mut stream = [0; 64];
        let len = match coding.id() {
            CodingId::PIXEL => pixel.encode_into(&samples, &mut stream).unwrap(),
            CodingId::RLE => rle.encode_into(&samples, &mut stream).unwrap(),
            _ => lz4
                .encoder(&mut table)
                .unwrap()
                .encode_into(&samples, &mut stream)
                .unwrap(),
        };
        let asset = EncodedImageAsset::new(surface, coding, &stream[..len]);
        assert_eq!(asset.surface(), surface);
        assert_eq!(asset.codings().collect::<Vec<_>>(), &[coding]);
        assert_eq!(asset.data(), &stream[..len]);
        assert_eq!(asset.input_alignment(), Ok(1));
        assert_eq!(asset.color_table(), None);
        assert_eq!(asset.encoded_len(), Ok(92 + coding.params().len() + len));
        let bytes = asset.encode().unwrap();
        assert_eq!(asset.matches_payload(&bytes), Ok(true));
        let view = EncodedImageView::open(&bytes).unwrap();
        assert_eq!(view.media().sections().count(), 3);
        assert_eq!(view.codings().get(0), Some(coding));
        view.validate_data().unwrap();
        let mut slots = [None];
        let groups = view
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert_eq!(groups.validate_unit(0, 0), Ok(len as u32));
        let unit = groups.get(0).unwrap().get(0).unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(64))
            .unwrap();
        let mut output = [0xad; 136];
        let decoded = plan.decode_into(&mut output).unwrap();
        let plane = decoded.plane(0).unwrap();
        assert_eq!(plane.memory().stride(), 64);
        assert_eq!(plane.row(0).unwrap(), Some(&samples[..9]));
        assert_eq!(plane.row(1).unwrap(), Some(&samples[9..]));
        assert_eq!(&output[9..64], &[0; 55]);
        assert_eq!(&output[73..128], &[0; 55]);
        assert_eq!(&output[128..], &[0xad; 8]);
    }
}

#[test]
fn stored_input_alignment_and_actual_addresses_are_distinct() {
    #[repr(align(64))]
    struct Aligned([u8; 512]);
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let stream = [0x83, 17];
    let asset =
        EncodedImageAsset::new(surface, Rle::new().record(), &stream).with_input_alignment(64);
    let mut bytes = Aligned([0xad; 512]);
    let len = asset.encode_into(&mut bytes.0).unwrap();
    assert_eq!(len, 198);
    assert_eq!(asset.matches_payload(&bytes.0[..len]), Ok(true));
    assert!(bytes.0[len..].iter().all(|b| *b == 0xad));
    let view = EncodedImageView::open_at(&bytes.0[..len], 0).unwrap();
    assert_eq!(view.media().sections().count(), 4);
    let data = view
        .media()
        .sections()
        .find(|s| s.descriptor().kind() == MediaSectionKind::DATA)
        .unwrap();
    assert_eq!(data.descriptor().offset(), 192);
    assert_eq!(&bytes.0[136..192], &[0; 56]);
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert!(
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .data_address_is_aligned()
    );
    view.validate_data().unwrap();
    let unaligned = EncodedImageView::open_at(&bytes.0[..len], 1).unwrap();
    assert!(matches!(
        unaligned.groups_into(&mut slots, &mut CoverageBudget::new(100)),
        Err(EncodedImageError::FileAddressUnaligned { alignment: 64, .. })
    ));
    // The writer accepts byte slices; serialization cannot align their allocation.
    let mut shifted = Aligned([0xad; 512]);
    asset.encode_into(&mut shifted.0[1..]).unwrap();
    let view = EncodedImageView::open(&shifted.0[1..1 + len]).unwrap();
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert!(
        !groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .data_address_is_aligned()
    );
}

#[test]
fn palette_and_yuv_metadata_remain_outside_the_coded_stream() {
    let palette = [0x44; 16];
    let indexed = SurfaceDescriptor::new(4, 1, SampleLayout::I2, ColorDescription::SRGB).unwrap();
    let asset = EncodedImageAsset::new(indexed, Rle::new().record(), &[0, 0x1b]);
    assert_eq!(
        asset.encoded_len(),
        Err(ImageEncodeError::MissingColorTable)
    );
    assert!(matches!(
        asset.with_color_table(&palette[..12]).encoded_len(),
        Err(ImageEncodeError::ColorTableLengthMismatch {
            expected: 16,
            actual: 12
        })
    ));
    let bytes = asset.with_color_table(&palette).encode().unwrap();
    let view = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(view.color_table().unwrap().len(), 4);
    assert_eq!(bytes.len(), 92 + 12 + palette.len() + 2);
    view.validate_data().unwrap();
    let yuv = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let asset = EncodedImageAsset::new(yuv, Rle::new().record(), &[0x90, 128]);
    assert_eq!(
        asset.with_color_table(&palette).encoded_len(),
        Err(ImageEncodeError::UnexpectedColorTable)
    );
    let bytes = asset.encode().unwrap();
    let view = EncodedImageView::open(&bytes).unwrap();
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    let plan = groups
        .get(0)
        .unwrap()
        .get(0)
        .unwrap()
        .decode_plan(SurfaceRequirements::new())
        .unwrap();
    let mut output = [0; 17];
    let decoded = plan.decode_into(&mut output).unwrap();
    assert_eq!(decoded.plane(0).unwrap().bytes(), &[128; 9]);
    assert_eq!(decoded.plane(1).unwrap().bytes(), &[128; 8]);
}

#[test]
fn metadata_authoring_preserves_unknown_profiles_without_promising_decodability() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let coding = CodingRecord::new(CodingId::new(511), 7, &[3, 1, 4]);
    let asset = EncodedImageAsset::new(surface, coding, &[0xfe]);
    let bytes = asset.encode().unwrap();
    let view = EncodedImageView::open(&bytes).unwrap();
    assert_eq!(view.codings().get(0), Some(coding));
    view.validate_data().unwrap();
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert_eq!(
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new()),
        Err(UnitDecodeError::UnsupportedCoding(coding.id()))
    );
    let malformed = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff])
        .encode()
        .unwrap();
    let view = EncodedImageView::open(&malformed).unwrap();
    view.validate_data().unwrap();
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert!(matches!(
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new()),
        Err(UnitDecodeError::Rle(_))
    ));
}

#[test]
fn omitted_empty_groups_and_preflight_errors_preserve_output() {
    let empty =
        SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(empty, Lz4::new().record(), &[]).with_input_alignment(64);
    assert_eq!(asset.encoded_len(), Ok(92));
    let bytes = asset.encode().unwrap();
    let view = EncodedImageView::open(&bytes).unwrap();
    let mut slots = [None];
    let groups = view
        .groups_into(&mut slots, &mut CoverageBudget::new(100))
        .unwrap();
    assert!(groups.get(0).unwrap().is_empty());
    let mut output = [0xad; 256];
    for invalid in [
        EncodedImageAsset::new(empty, Lz4::new().record(), &[0]),
        EncodedImageAsset::new(empty, CodingRecord::new(CodingId::RAW, 1, &[]), &[]),
        asset.with_input_alignment(3),
    ] {
        assert!(invalid.encode_into(&mut output).is_err());
        assert_eq!(output, [0xad; 256]);
    }
    assert_eq!(
        asset.encode_into(&mut output[..91]),
        Err(ImageEncodeError::BufferTooSmall {
            needed: 92,
            available: 91
        })
    );
    assert_eq!(output, [0xad; 256]);
    asset.encode_into(&mut output).unwrap();
    assert_eq!(&output[..92], &bytes);
    assert_eq!(&output[92..], &[0xad; 164]);
    assert_eq!(asset.matches_payload(&output[..91]), Ok(false));
    for i in 0..92 {
        output[i] ^= 1;
        assert_eq!(asset.matches_payload(&output[..92]), Ok(false));
        output[i] ^= 1;
    }
}
