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
