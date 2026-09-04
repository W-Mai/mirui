use super::*;
use alloc::{vec, vec::Vec};

#[test]
fn element_size_and_default_omission_have_one_canonical_record() {
    assert_eq!(Rle::default(), Rle::new());
    for element in 1..=4 {
        let codec = Rle::new().with_element_size(element).unwrap();
        assert_eq!(codec.element_size(), element);
        let record = codec.record();
        assert_eq!(record.id(), CodingId::RLE);
        assert_eq!(record.revision(), 1);
        assert_eq!(
            record.params(),
            if element == 1 {
                &[][..]
            } else {
                core::slice::from_ref(&element)
            }
        );
        assert_eq!(Rle::from_record(record), Ok(codec));
    }
    for size in [0, 5, 255] {
        assert_eq!(
            Rle::new().with_element_size(size),
            Err(RleError::InvalidElementSize(size))
        );
    }
    for params in [&[0][..], &[1], &[5], &[2, 0]] {
        assert_eq!(
            Rle::from_record(CodingRecord::new(CodingId::RLE, 1, params)),
            Err(RleError::UnexpectedParameters)
        );
    }
    assert_eq!(
        Rle::from_record(CodingRecord::new(CodingId::PIXEL, 1, &[])),
        Err(RleError::UnexpectedCoding(CodingId::PIXEL))
    );
    assert_eq!(
        Rle::from_record(CodingRecord::new(CodingId::RLE, 2, &[])),
        Err(RleError::UnsupportedRevision(2))
    );
}

#[test]
fn canonical_encoding_selects_actual_size_and_stable_ties() {
    let codec = Rle::new();
    for (input, expected) in [
        (&[1, 1][..], &[0x81, 1][..]),
        (&[1, 1, 2][..], &[0x81, 1, 0, 2][..]),
        (&[1, 1, 2, 3, 3, 4][..], &[5, 1, 1, 2, 3, 3, 4][..]),
    ] {
        let mut out = [0xad; 16];
        assert_eq!(codec.encoded_len(input), Ok(expected.len()));
        assert_eq!(codec.encode_into(input, &mut out), Ok(expected.len()));
        assert_eq!(&out[..expected.len()], expected);
        assert!(out[expected.len()..].iter().all(|b| *b == 0xad));
    }
    let codec = codec.with_element_size(2).unwrap();
    let samples = [1, 2, 1, 2, 1, 3, 3, 1];
    let expected = [0x81, 1, 2, 1, 1, 3, 3, 1];
    let mut encoded = [0; 16];
    assert_eq!(
        codec.encode_into(&samples, &mut encoded),
        Ok(expected.len())
    );
    assert_eq!(&encoded[..expected.len()], expected);
    let plan = codec.plan(&expected, samples.len()).unwrap();
    assert_eq!(plan.codec(), codec);
    assert_eq!(plan.input(), expected);
    assert_eq!(plan.decoded_len(), samples.len());
    let mut out = [0xad; 12];
    assert_eq!(plan.decode_into(&mut out), Ok(samples.len()));
    assert_eq!(&out[..8], samples);
    assert_eq!(&out[8..], &[0xad; 4]);
}

#[test]
fn every_control_is_positive_bounded_and_independent() {
    for element in 1..=4 {
        let codec = Rle::new().with_element_size(element).unwrap();
        for control in 0..=255u8 {
            let count = usize::from(control & 127) + 1;
            let literal = control & 128 == 0;
            let body = if literal {
                count * usize::from(element)
            } else {
                usize::from(element)
            };
            let mut input = vec![control];
            input.extend((0..body).map(|i| i as u8));
            let mut output = vec![0; count * usize::from(element)];
            codec
                .plan(&input, output.len())
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
            if literal {
                assert_eq!(output, input[1..]);
            } else {
                for value in output.chunks_exact(usize::from(element)) {
                    assert_eq!(value, &input[1..]);
                }
            }
        }
    }
    let codec = Rle::new();
    let mut encoded = [0; 8];
    assert_eq!(codec.encode_into(&[9; 129], &mut encoded), Ok(4));
    assert_eq!(&encoded[..4], &[255, 9, 0, 9]);
    let literal: Vec<_> = (0..128).collect();
    let mut encoded = [0; 129];
    assert_eq!(codec.encode_into(&literal, &mut encoded), Ok(129));
    assert_eq!(encoded[0], 127);
}

#[test]
fn count_truncation_overflow_and_capacity_errors_precede_writes() {
    let codec = Rle::new();
    let mut out = [0xad; 8];
    assert!(codec.encode_into(&[1, 2, 3], &mut out[..3]).is_err());
    assert_eq!(out, [0xad; 8]);
    assert!(
        codec
            .plan(&[2, 1, 2, 3], 3)
            .unwrap()
            .decode_into(&mut out[..2])
            .is_err()
    );
    assert_eq!(out, [0xad; 8]);
    assert!(matches!(
        codec.plan(&[0x81, 3], 1),
        Err(RleError::OutputOverflow { .. })
    ));
    assert!(matches!(
        codec.plan(&[0x81], 2),
        Err(RleError::Truncated { .. })
    ));
    assert!(matches!(
        codec.plan(&[1, 3], 2),
        Err(RleError::Truncated { .. })
    ));
    assert!(matches!(
        codec.plan(&[0, 3, 0], 1),
        Err(RleError::TrailingData { .. })
    ));
    assert_eq!(codec.encoded_bound(usize::MAX), Err(RleError::SizeOverflow));
    let two = codec.with_element_size(2).unwrap();
    assert!(matches!(
        two.plan(&[], 1),
        Err(RleError::PartialElement { .. })
    ));
    assert!(matches!(
        two.encoded_len(&[1]),
        Err(RleError::PartialElement { .. })
    ));
    assert!(matches!(
        two.encode_into(&[1], &mut out),
        Err(RleError::PartialElement { .. })
    ));
    assert_eq!(out, [0xad; 8]);
    assert_eq!(codec.encoded_len(&[]), Ok(0));
    assert_eq!(codec.encode_into(&[], &mut out), Ok(0));
    assert_eq!(codec.plan(&[], 0).unwrap().decode_into(&mut out), Ok(0));
    assert_eq!(out, [0xad; 8]);
}
