use super::*;
use alloc::{vec, vec::Vec};

struct Block(Vec<u8>);
impl Block {
    fn length(&mut self, len: usize) {
        if len >= 15 {
            let mut remaining = len - 15;
            while remaining >= 255 {
                self.0.push(255);
                remaining -= 255;
            }
            self.0.push(remaining as u8);
        }
    }
    fn sequence(&mut self, literals: &[u8], match_: Option<(u16, usize)>) {
        let len = match_.map_or(0, |(_, len)| len - 4);
        self.0
            .push((literals.len().min(15) as u8) << 4 | len.min(15) as u8);
        self.length(literals.len());
        self.0.extend_from_slice(literals);
        if let Some((distance, _)) = match_ {
            self.0.extend_from_slice(&distance.to_le_bytes());
            self.length(len);
        }
    }
}

#[test]
fn profile_has_one_empty_parameter_form_and_no_dictionary() {
    let codec = Lz4::new();
    let record = codec.record();
    assert_eq!(record, CodingRecord::new(CodingId::LZ4, 1, &[]));
    assert_eq!(Lz4::from_record(record), Ok(codec));
    assert_eq!(
        Lz4::from_record(CodingRecord::new(CodingId::RAW, 1, &[])),
        Err(Lz4Error::UnexpectedCoding(CodingId::RAW))
    );
    assert_eq!(
        Lz4::from_record(CodingRecord::new(CodingId::LZ4, 2, &[])),
        Err(Lz4Error::UnsupportedRevision(2))
    );
    assert_eq!(
        Lz4::from_record(CodingRecord::new(CodingId::LZ4, 1, &[0])),
        Err(Lz4Error::UnexpectedParameters)
    );
    assert!(matches!(
        codec.plan(&[0, 1, 0], 4),
        Err(Lz4Error::InvalidOffset { .. })
    ));
    assert!(matches!(
        codec.plan(&[0, 0, 0], 4),
        Err(Lz4Error::InvalidOffset { .. })
    ));
}

#[test]
fn literal_lengths_and_overlapping_matches_preserve_exact_bytes() {
    let codec = Lz4::new();
    for len in [0, 1, 14, 15, 16, 269, 270, 271, 65535] {
        let literals: Vec<_> = (0..len).map(|i| i as u8).collect();
        let mut block = Block(vec![]);
        block.sequence(&literals, None);
        let plan = codec.plan(&block.0, len).unwrap();
        assert_eq!(plan.codec(), codec);
        assert_eq!(plan.input(), block.0);
        assert_eq!(plan.decoded_len(), len);
        let mut output = vec![0xad; len + 1];
        assert_eq!(plan.decode_into(&mut output), Ok(len));
        assert_eq!(output[..len], literals);
        assert_eq!(output[len], 0xad);
    }
    for distance in [1u16, 2, 3, 4, 17, 65535] {
        for len in [7, 18, 19, 20, 273, 274, 275, 1024] {
            let literals: Vec<_> = (0..distance).map(|i| i as u8).collect();
            let mut block = Block(vec![]);
            block.sequence(&literals, Some((distance, len)));
            block.sequence(b"tail!", None);
            let mut expected = literals.clone();
            for i in 0..len {
                expected.push(literals[i % usize::from(distance)]);
            }
            expected.extend_from_slice(b"tail!");
            let mut output = vec![0xad; expected.len() + 1];
            codec
                .plan(&block.0, expected.len())
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
            assert_eq!(output[..expected.len()], expected);
            assert_eq!(output[expected.len()], 0xad);
        }
    }
}

#[test]
fn block_end_limits_are_independent_of_output_capacity() {
    let codec = Lz4::new();
    for tail in 0..=6 {
        for matched in 4..=13 {
            let mut block = Block(vec![]);
            block.sequence(b"a", Some((1, matched)));
            block.sequence(&vec![b'z'; tail], None);
            let result = codec.plan(&block.0, 1 + matched + tail);
            assert_eq!(result.is_ok(), tail >= 5 && matched + tail >= 12);
        }
    }
    for low in 1..=15 {
        assert_eq!(codec.plan(&[low], 0), Err(Lz4Error::InvalidBlockEnd));
    }
    assert_eq!(codec.plan(&[], 0), Err(Lz4Error::Truncated { offset: 0 }));
    assert_eq!(codec.plan(&[0], 0).unwrap().decode_into(&mut []), Ok(0));
    let mut block = Block(vec![]);
    block.sequence(b"a", Some((1, 12)));
    assert!(matches!(
        codec.plan(&block.0, 13),
        Err(Lz4Error::Truncated { .. })
    ));
}

#[test]
fn truncation_counts_and_capacity_are_checked_before_output_writes() {
    let codec = Lz4::new();
    let block = [0x13, b'a', 1, 0, 0x50, b't', b'a', b'i', b'l', b'!'];
    for len in 0..block.len() {
        assert!(codec.plan(&block[..len], 13).is_err());
    }
    for len in [0, 1, 12, 14, usize::MAX] {
        assert!(codec.plan(&block, len).is_err());
    }
    for input in [
        &[0xf0, 255][..],
        &[0x1f, b'a', 1, 0, 255][..],
        &[0x10, b'a', 0][..],
    ] {
        assert!(matches!(
            codec.plan(input, 4096),
            Err(Lz4Error::Truncated { .. })
        ));
    }
    let plan = codec.plan(&block, 13).unwrap();
    let mut output = [0xad; 16];
    assert_eq!(
        plan.decode_into(&mut output[..12]),
        Err(Lz4Error::OutputTooSmall {
            needed: 13,
            available: 12
        })
    );
    assert_eq!(output, [0xad; 16]);
    assert_eq!(plan.decode_into(&mut output), Ok(13));
    assert_eq!(&output[..13], b"aaaaaaaatail!");
    assert_eq!(&output[13..], &[0xad; 3]);
}

#[test]
fn encoder_workspace_is_explicit_bounded_and_reset_per_pass() {
    let codec = Lz4::new();
    for entries in [0, 1, 15, 17, 1000, 65537, 131072] {
        assert!(matches!(
            codec.encoder(&mut vec![0; entries]),
            Err(Lz4Error::InvalidWorkspace { .. })
        ));
    }
    for entries in [16, 64, 256, Lz4::TABLE_LEN, 4096, 65536] {
        let mut table = vec![u32::MAX; entries];
        let mut encoder = codec.encoder(&mut table).unwrap();
        assert_eq!(encoder.table_len(), entries);
        let mut encoded = [0xad; 20];
        let expected = [0x13, b'a', 1, 0, 0x50, b'a', b'a', b'a', b'a', b'a'];
        assert_eq!(encoder.encoded_len(&[b'a'; 13]), Ok(expected.len()));
        assert_eq!(
            encoder.encode_into(&[b'a'; 13], &mut encoded[..9]),
            Err(Lz4Error::OutputTooSmall {
                needed: 10,
                available: 9
            })
        );
        assert_eq!(encoded, [0xad; 20]);
        encoder
            .encode_into(b"xyzxyzxyzxyzxyzxyz", &mut encoded)
            .unwrap();
        encoded.fill(0xad);
        assert_eq!(encoder.encode_into(&[b'a'; 13], &mut encoded), Ok(10));
        assert_eq!(&encoded[..10], &expected);
        assert_eq!(&encoded[10..], &[0xad; 10]);
        assert_eq!(encoder.encode_into(&[], &mut encoded), Ok(1));
        assert_eq!(encoded[0], 0);
        assert_eq!(encoder.encode_into(b"abc", &mut encoded), Ok(4));
        assert_eq!(&encoded[..4], &[0x30, b'a', b'b', b'c']);
    }
    assert_eq!(codec.encoded_bound(0), Ok(16));
    assert!(codec.encoded_bound(usize::MAX).is_err());
    if let Ok(len) = usize::try_from(u64::from(u32::MAX) + 1) {
        assert_eq!(
            codec.encoded_bound(len),
            Err(Lz4Error::InputTooLarge { bytes: len })
        );
    }
}

#[test]
fn encoder_extensions_windows_and_bounds_share_the_decoder_contract() {
    let codec = Lz4::new();
    let mut table = [0; Lz4::TABLE_LEN];
    let mut encoder = codec.encoder(&mut table).unwrap();
    for len in [
        0, 1, 12, 13, 14, 15, 16, 269, 270, 271, 65534, 65535, 65536, 131075,
    ] {
        for period in [1, 3, 17, 251] {
            let input: Vec<_> = (0..len).map(|i| (i % period) as u8).collect();
            let size = encoder.encoded_len(&input).unwrap();
            assert!(size <= codec.encoded_bound(len).unwrap());
            let mut encoded = vec![0xad; size + 1];
            assert_eq!(encoder.encode_into(&input, &mut encoded), Ok(size));
            assert_eq!(encoded[size], 0xad);
            let mut output = vec![0; len];
            codec
                .plan(&encoded[..size], len)
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
            assert_eq!(output, input);
        }
    }

    let mut input = vec![42; 65536];
    input[..4].copy_from_slice(&[1, 2, 3, 4]);
    input.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
    let mut encoded = vec![0; encoder.encoded_len(&input).unwrap()];
    encoder.encode_into(&input, &mut encoded).unwrap();
    let mut output = vec![0; input.len()];
    codec
        .plan(&encoded, input.len())
        .unwrap()
        .decode_into(&mut output)
        .unwrap();
    assert_eq!(output, input);
}
