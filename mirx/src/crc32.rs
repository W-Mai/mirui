//! CRC-32/IEEE (a.k.a. CRC-32/ISO-HDLC), reflected.
//! Polynomial `0xEDB88320`, init `0xFFFFFFFF`, xorout `0xFFFFFFFF`.
//! Used by the FLAT and CHUNK file headers; matches `crc32fast` 1.x output.

const POLY: u32 = 0xEDB88320;

const TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut bit = 0;
        while bit < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLY;
            } else {
                crc >>= 1;
            }
            bit += 1;
        }
        t[i as usize] = crc;
        i += 1;
    }
    t
}

pub fn compute(buf: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    let mut i = 0;
    while i < buf.len() {
        let idx = ((crc ^ buf[i] as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ TABLE[idx];
        i += 1;
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_is_zero() {
        assert_eq!(compute(&[]), 0);
    }

    #[test]
    fn matches_known_vectors() {
        // From the standard CRC-32/IEEE test vectors.
        assert_eq!(compute(b"123456789"), 0xCBF43926);
        assert_eq!(compute(b"a"), 0xE8B7BE43);
        assert_eq!(compute(b"abc"), 0x352441C2);
    }

    #[test]
    fn table_first_and_last_entries() {
        assert_eq!(TABLE[0], 0);
        assert_eq!(TABLE[1], 0x77073096);
        assert_eq!(TABLE[255], 0x2D02EF8D);
    }
}
