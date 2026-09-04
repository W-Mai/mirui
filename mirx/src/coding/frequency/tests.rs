use super::*;
use alloc::{vec, vec::Vec};

fn samples(geometry: FrequencyGeometry) -> Vec<u8> {
    (0..geometry.decoded_len().unwrap())
        .map(|index| ((index * 73 + index / 7 * 19 + 11) & 0xff) as u8)
        .collect()
}

fn encoded(codec: Frequency, geometry: FrequencyGeometry, samples: &[u8]) -> Vec<u8> {
    let needed = codec.encoded_len(geometry, samples).unwrap();
    assert!(needed <= codec.encoded_bound(geometry).unwrap());
    let mut bytes = vec![0xa5; needed + 3];
    let len = codec.encode_into(geometry, samples, &mut bytes).unwrap();
    assert_eq!(len, needed);
    assert_eq!(&bytes[len..], &[0xa5; 3]);
    bytes.truncate(len);
    bytes
}

#[test]
fn records_keep_loss_policy_and_quality_explicit() {
    let mut params = [0xa5];
    let reversible = Frequency::reversible();
    assert_eq!(reversible.record_into(&mut params).params(), &[]);
    assert_eq!(params, [0xa5]);
    assert_eq!(
        Frequency::from_record(CodingRecord::new(CodingId::FREQUENCY_REVERSIBLE, 1, &[])),
        Ok(reversible)
    );

    for quality in [1, 50, 100] {
        let codec = Frequency::quantized(quality).unwrap();
        let mut params = [0];
        let record = codec.record_into(&mut params);
        assert_eq!(record.id(), CodingId::FREQUENCY_QUANTIZED);
        assert_eq!(record.params(), &[quality]);
        assert_eq!(Frequency::from_record(record), Ok(codec));
    }
    for quality in [0, 101, u8::MAX] {
        assert_eq!(
            Frequency::quantized(quality),
            Err(FrequencyError::InvalidQuality(quality))
        );
    }
    for record in [
        CodingRecord::new(CodingId::RAW, 1, &[]),
        CodingRecord::new(CodingId::FREQUENCY_REVERSIBLE, 2, &[]),
        CodingRecord::new(CodingId::FREQUENCY_REVERSIBLE, 1, &[1]),
        CodingRecord::new(CodingId::FREQUENCY_QUANTIZED, 1, &[]),
        CodingRecord::new(CodingId::FREQUENCY_QUANTIZED, 1, &[50, 0]),
    ] {
        assert!(Frequency::from_record(record).is_err());
    }
}

#[test]
fn reversible_profile_roundtrips_every_admitted_layout_and_odd_edge() {
    let cases = [
        (SampleLayout::I8, 0),
        (SampleLayout::A8, 0),
        (SampleLayout::L8, 0),
        (SampleLayout::RGB888, 0),
        (SampleLayout::RGBA8888, 0),
        (SampleLayout::BGRA8888, 0),
        (SampleLayout::I420, 0),
        (SampleLayout::I420, 1),
        (SampleLayout::I420, 2),
        (SampleLayout::YV12, 2),
        (SampleLayout::NV12, 0),
        (SampleLayout::NV12, 1),
        (SampleLayout::NV21, 1),
    ];
    for (layout, plane) in cases {
        let geometry = FrequencyGeometry::for_plane(layout, plane, 11, 9).unwrap();
        let source = samples(geometry);
        let codec = Frequency::reversible();
        let bytes = encoded(codec, geometry, &source);
        assert!(bytes.len() <= codec.encoded_bound(geometry).unwrap());
        let plan = codec.plan(&bytes, geometry).unwrap();
        assert_eq!(plan.geometry(), geometry);
        assert_eq!(plan.input(), &bytes);
        let mut output = vec![0xa5; source.len() + 3];
        assert_eq!(plan.decode_into(&mut output), Ok(source.len()));
        assert_eq!(&output[..source.len()], source);
        assert_eq!(&output[source.len()..], &[0xa5; 3]);
    }
}

#[test]
fn empty_planes_have_one_canonical_empty_stream() {
    for (width, height) in [(0, 0), (0, 5), (7, 0)] {
        let geometry =
            FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, width, height).unwrap();
        let codec = Frequency::reversible();
        assert_eq!(codec.encoded_bound(geometry), Ok(0));
        assert_eq!(codec.encoded_len(geometry, &[]), Ok(0));
        assert_eq!(codec.plan(&[], geometry).unwrap().decoded_len(), 0);
        assert_eq!(
            codec.plan(&[0], geometry),
            Err(FrequencyError::TrailingData { offset: 0 })
        );
    }
}

#[test]
fn quantized_color_is_bounded_while_alpha_and_indexes_remain_exact() {
    let geometry = FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 17, 10).unwrap();
    let source = samples(geometry);
    for quality in [1, 40, 80, 100] {
        let codec = Frequency::quantized(quality).unwrap();
        let bytes = encoded(codec, geometry, &source);
        let plan = codec.plan(&bytes, geometry).unwrap();
        let mut output = vec![0; source.len()];
        plan.decode_into(&mut output).unwrap();
        for (before, after) in source.chunks_exact(4).zip(output.chunks_exact(4)) {
            assert_eq!(before[3], after[3], "quality {quality}");
            for channel in 0..3 {
                assert!(
                    before[channel].abs_diff(after[channel]) <= 52,
                    "quality {quality}, {before:?} -> {after:?}"
                );
            }
        }
        assert_ne!(&source[..source.len() - 1], &output[..output.len() - 1]);
    }

    let indexes = FrequencyGeometry::for_plane(SampleLayout::I8, 0, 13, 5).unwrap();
    let source = samples(indexes);
    let codec = Frequency::quantized(1).unwrap();
    let bytes = encoded(codec, indexes, &source);
    let mut output = vec![0; source.len()];
    codec
        .plan(&bytes, indexes)
        .unwrap()
        .decode_into(&mut output)
        .unwrap();
    assert_eq!(output, source);
}

#[test]
fn quantized_profile_is_deterministic_and_quality_100_is_not_lossless() {
    let geometry = FrequencyGeometry::for_plane(SampleLayout::L8, 0, 9, 9).unwrap();
    let source = samples(geometry);
    let codec = Frequency::quantized(100).unwrap();
    let first = encoded(codec, geometry, &source);
    let second = encoded(codec, geometry, &source);
    assert_eq!(first, second);
    let mut output = vec![0; source.len()];
    codec
        .plan(&first, geometry)
        .unwrap()
        .decode_into(&mut output)
        .unwrap();
    assert_ne!(output, source);
}

#[test]
fn quantized_edge_extension_preserves_constant_surfaces() {
    let geometry = FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 13, 9).unwrap();
    let source = [17, 42, 91, 137].repeat(13 * 9);
    for quality in [1, 25, 50, 75, 100] {
        let codec = Frequency::quantized(quality).unwrap();
        let stream = encoded(codec, geometry, &source);
        let plan = codec.plan(&stream, geometry).unwrap();
        let mut output = vec![0; source.len()];
        plan.decode_into(&mut output).unwrap();
        assert_eq!(output, source, "quality {quality}");
    }
}

#[test]
fn syntax_validation_rejects_truncation_overflow_and_noncanonical_values() {
    let geometry = FrequencyGeometry::for_plane(SampleLayout::A8, 0, 1, 1).unwrap();
    let codec = Frequency::reversible();
    let source = [42];
    let bytes = encoded(codec, geometry, &source);
    assert_eq!(bytes, [128, 171, 1, 62]);
    for end in 0..bytes.len() {
        assert!(codec.plan(&bytes[..end], geometry).is_err(), "end {end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        codec.plan(&trailing, geometry),
        Err(FrequencyError::TrailingData {
            offset: bytes.len()
        })
    );
    assert_eq!(
        codec.plan(&[0x7f], geometry),
        Err(FrequencyError::CoefficientRunOverflow {
            remaining: 64,
            count: 128
        })
    );
    assert_eq!(
        codec.plan(&[0x80, 0], geometry),
        Err(FrequencyError::LiteralZero)
    );
    assert_eq!(
        codec.plan(&[0x80, 0x81, 0], geometry),
        Err(FrequencyError::NonCanonicalVarint)
    );
    assert_eq!(
        codec.plan(&[0x80, 0xff, 0xff, 0xff, 0xff, 0x10], geometry),
        Err(FrequencyError::VarintOverflow)
    );
}

#[test]
fn sizing_and_layout_errors_are_atomic() {
    for (layout, plane) in [
        (SampleLayout::I4, 0),
        (SampleLayout::RGB565, 0),
        (SampleLayout::XRGB8888, 0),
        (SampleLayout::P010, 0),
        (SampleLayout::RGBA8888, 1),
    ] {
        assert!(matches!(
            FrequencyGeometry::for_plane(layout, plane, 1, 1),
            Err(FrequencyError::UnsupportedLayout { .. })
        ));
    }

    let geometry = FrequencyGeometry::for_plane(SampleLayout::RGB888, 0, 3, 2).unwrap();
    let source = samples(geometry);
    let codec = Frequency::reversible();
    assert_eq!(
        codec.encoded_len(geometry, &source[..source.len() - 1]),
        Err(FrequencyError::SampleLengthMismatch {
            expected: source.len(),
            actual: source.len() - 1
        })
    );
    let needed = codec.encoded_len(geometry, &source).unwrap();
    let mut output = vec![0xa5; needed.saturating_sub(1)];
    assert!(matches!(
        codec.encode_into(geometry, &source, &mut output),
        Err(FrequencyError::OutputTooSmall { .. })
    ));
    assert!(output.iter().all(|byte| *byte == 0xa5));

    let bytes = encoded(codec, geometry, &source);
    let plan = codec.plan(&bytes, geometry).unwrap();
    let mut decoded = vec![0xa5; source.len() - 1];
    assert!(matches!(
        plan.decode_into(&mut decoded),
        Err(FrequencyError::OutputTooSmall { .. })
    ));
    assert!(decoded.iter().all(|byte| *byte == 0xa5));
}

#[test]
fn lifting_and_signed_mapping_are_exact_at_numeric_edges() {
    for a in [-4096, -255, -1, 0, 1, 255, 4096] {
        for b in [-4096, -255, -1, 0, 1, 255, 4096] {
            let mut values = [a, b, 3, -7, 19, -23, 127, -128];
            let expected = values;
            forward_1d(&mut values);
            inverse_1d(&mut values);
            assert_eq!(values, expected);
        }
    }
    for value in [i32::MIN, -1_000_000, -1, 0, 1, 1_000_000, i32::MAX] {
        assert_eq!(unzigzag(zigzag(value)), value);
    }
}
