use super::*;
use alloc::{vec, vec::Vec};

#[test]
fn aligned_length_forms_keep_exact_ranges_and_bounded_bidirectional_access() {
    for count in [0, 1, 63, 64, 65, 129, 4097] {
        for alignment_bytes in [1, 2, 64, 4096] {
            let alignment = alignment(alignment_bytes);
            for encoding in [UnitIndexEncoding::Lengths16, UnitIndexEncoding::Lengths32] {
                let lengths: Vec<_> = (0..count)
                    .map(|i| {
                        (i * 101)
                            % if encoding == UnitIndexEncoding::Lengths16 {
                                65536
                            } else {
                                100003
                            }
                    })
                    .collect();
                let mut expected = Vec::new();
                let mut end = 0u64;
                for &len in &lengths {
                    let start =
                        end.div_ceil(u64::from(alignment.get())) * u64::from(alignment.get());
                    end = start + u64::from(len);
                    expected.push(start as u32..end as u32);
                }
                let mut bytes = vec![0xad; encoding.encoded_len(&lengths, alignment).unwrap() + 2];
                let len = encoding
                    .encode_into(&lengths, alignment, &mut bytes[1..])
                    .unwrap();
                assert_eq!(bytes[0], 0xad);
                assert_eq!(bytes[len + 1], 0xad);
                let index = match encoding {
                    UnitIndexEncoding::Lengths16 => {
                        UnitIndex::lengths16(count, &bytes[1..len + 1], alignment)
                    }
                    UnitIndexEncoding::Lengths32 => {
                        UnitIndex::lengths32(count, &bytes[1..len + 1], alignment)
                    }
                    _ => unreachable!(),
                }
                .unwrap();
                assert_eq!(index.byte_len(), end as u32);
                assert!(index.iter().eq(expected.iter().cloned()));
                assert!(index.iter().rev().eq(expected.iter().rev().cloned()));
                for (i, range) in expected.iter().enumerate() {
                    assert_eq!(index.get(i), Some(range.clone()));
                }
                let mut actual = index.iter();
                let mut reference = expected.iter().cloned();
                assert_eq!(actual.next(), reference.next());
                assert_eq!(actual.next_back(), reference.next_back());
                assert_eq!(actual.nth(62), reference.nth(62));
                assert_eq!(actual.nth_back(62), reference.nth_back(62));
                assert!(actual.eq(reference));
                let mut exhausted = index.iter();
                assert_eq!(exhausted.nth_back(usize::MAX), None);
                assert_eq!(exhausted.next(), None);
            }
        }
    }
}

#[test]
fn fixed_gaps_and_last_u32_end_do_not_require_a_trailing_padding_span() {
    let fixed = UnitIndex::fixed(3, 5, alignment(64)).unwrap();
    assert_eq!(fixed.byte_len(), 133);
    assert!(fixed.iter().eq([0..5, 64..69, 128..133]));
    assert!(fixed.iter().rev().eq([128..133, 64..69, 0..5]));
    assert!(
        UnitIndex::fixed(0, u32::MAX, alignment(1 << 31))
            .unwrap()
            .is_empty()
    );
    let last = UnitIndex::fixed(1, u32::MAX, alignment(64)).unwrap();
    assert_eq!(last.iter().next(), Some(0..u32::MAX));
    assert_eq!(last.iter().next_back(), Some(0..u32::MAX));
    let separated = UnitIndex::fixed(2, 1, alignment(1 << 31)).unwrap();
    assert!(separated.iter().rev().eq([(1 << 31)..(1 << 31) + 1, 0..1]));
    assert_eq!(
        UnitIndex::fixed(3, 1, alignment(1 << 31)),
        Err(UnitIndexError::SizeOverflow)
    );
    let mut bytes = [0; 8];
    UnitIndexEncoding::Lengths32
        .encode_into(&[u32::MAX], alignment(64), &mut bytes)
        .unwrap();
    let last = UnitIndex::lengths32(1, &bytes, alignment(64)).unwrap();
    assert_eq!(last.iter().next(), Some(0..u32::MAX));
    assert_eq!(last.iter().next_back(), Some(0..u32::MAX));
}

#[test]
fn alignment_width_overflow_and_noncanonical_checkpoints_are_atomic_errors() {
    let mut output = [0xad; 272];
    for bytes in [0, 3, u32::MAX] {
        assert!(ByteAlignment::new(bytes).is_err());
    }
    assert!(matches!(
        UnitIndexEncoding::Offsets.encode_into(&[3, 4], alignment(64), &mut output),
        Err(UnitIndexError::UnalignedOffset {
            index: 1,
            offset: 3,
            alignment: actual
        }) if actual == 64
    ));
    assert!(
        UnitIndexEncoding::Lengths16
            .encode_into(&[65536], alignment(64), &mut output)
            .is_err()
    );
    assert!(
        UnitIndexEncoding::Lengths32
            .encode_into(&[u32::MAX, 0], alignment(64), &mut output)
            .is_err()
    );
    assert!(
        UnitIndexEncoding::Lengths32
            .encode_into(&[1; 65], alignment(64), &mut output[..267])
            .is_err()
    );
    assert_eq!(output, [0xad; 272]);
    let len = UnitIndexEncoding::Lengths32
        .encode_into(&[1; 65], alignment(64), &mut output)
        .unwrap();
    assert_eq!(read_u32_le(&output, 4), Some(4096));
    assert!(UnitIndex::lengths32(65, &output[..len], alignment(1)).is_err());
    output[4] ^= 1;
    assert!(matches!(
        UnitIndex::lengths32(65, &output[..len], alignment(64)),
        Err(UnitIndexError::CheckpointMismatch { index: 64, .. })
    ));
}
