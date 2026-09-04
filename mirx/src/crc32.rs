//! CRC-32/IEEE (a.k.a. CRC-32/ISO-HDLC), reflected.
//! Polynomial `0xEDB88320`, init `0xFFFFFFFF`, xorout `0xFFFFFFFF`.
//! Used by the FLAT and CHUNK file headers and versioned payload envelopes;
//! matches `crc32fast` 1.x output.

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
    let mut crc = Crc32::new();
    crc.update(buf);
    crc.finish()
}

pub(crate) struct Crc32(u32);

impl Crc32 {
    pub(crate) const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let index = ((self.0 ^ u32::from(byte)) & 0xff) as usize;
            self.0 = (self.0 >> 8) ^ TABLE[index];
        }
    }

    pub(crate) const fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
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
    fn streaming_is_independent_of_segment_boundaries() {
        let bytes = b"123456789";
        for split in 0..=bytes.len() {
            let mut crc = Crc32::new();
            crc.update(&bytes[..split]);
            crc.update(&[]);
            crc.update(&bytes[split..]);
            assert_eq!(crc.finish(), compute(bytes));
        }
    }

    #[test]
    fn table_first_and_last_entries() {
        assert_eq!(TABLE[0], 0);
        assert_eq!(TABLE[1], 0x77073096);
        assert_eq!(TABLE[255], 0x2D02EF8D);
    }
}
