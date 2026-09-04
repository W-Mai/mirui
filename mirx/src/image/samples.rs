/// Copies validated MSB-first sample bits while preserving neighbouring bits.
/// Starts are intra-byte offsets; the caller has already checked both spans.
pub(super) fn copy(
    source: &[u8],
    mut source_bit: u8,
    target: &mut [u8],
    mut target_bit: u8,
    mut bits: u64,
) {
    debug_assert!(source_bit < 8 && target_bit < 8);
    debug_assert!(u64::from(source_bit) + bits <= source.len() as u64 * 8);
    debug_assert!(u64::from(target_bit) + bits <= target.len() as u64 * 8);
    let mut source_byte = 0;
    let mut target_byte = 0;
    while bits != 0 {
        if source_bit == 0 && target_bit == 0 && bits >= 8 {
            let bytes = (bits / 8) as usize;
            target[target_byte..target_byte + bytes]
                .copy_from_slice(&source[source_byte..source_byte + bytes]);
            source_byte += bytes;
            target_byte += bytes;
            bits %= 8;
            continue;
        }
        let take = bits.min(u64::from(8 - target_bit)) as u8;
        let mut value = source[source_byte] << source_bit;
        if source_bit + take > 8 {
            value |= source[source_byte + 1] >> (8 - source_bit);
        }
        let mask = (0xff >> (8 - take)) << (8 - target_bit - take);
        target[target_byte] = (target[target_byte] & !mask) | ((value >> target_bit) & mask);
        source_bit += take;
        source_byte += usize::from(source_bit / 8);
        source_bit %= 8;
        target_bit += take;
        target_byte += usize::from(target_bit / 8);
        target_bit %= 8;
        bits -= u64::from(take);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bit_offset_preserves_exact_source_samples_and_target_neighbours() {
        let source = [0x95, 0x3e, 0x76, 0xac, 0x5b, 0x13, 0xed, 0x22, 0xff];
        for source_bit in 0..8u8 {
            for target_bit in 0..8u8 {
                for bits in 0..=64u64 {
                    let mut target = [0xa5; 9];
                    let mut expected = target;
                    for bit in 0..bits as usize {
                        let s = usize::from(source_bit) + bit;
                        let t = usize::from(target_bit) + bit;
                        let value = source[s / 8] >> (7 - s % 8) & 1;
                        expected[t / 8] =
                            (expected[t / 8] & !(1 << (7 - t % 8))) | (value << (7 - t % 8));
                    }
                    copy(&source, source_bit, &mut target, target_bit, bits);
                    assert_eq!(
                        target, expected,
                        "source={source_bit}, target={target_bit}, bits={bits}"
                    );
                }
            }
        }
    }
}
