pub const FONT_CHUNK_HEADER_LEN: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontChunkKind {
    Grayscale,
    Sdf,
}

impl FontChunkKind {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Grayscale),
            1 => Some(Self::Sdf),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Grayscale => 0,
            Self::Sdf => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontChunkHeader {
    pub kind: FontChunkKind,
    pub format: u8,
    pub size: u16,
}

impl FontChunkHeader {
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < FONT_CHUNK_HEADER_LEN {
            return None;
        }
        Some(Self {
            kind: FontChunkKind::from_u8(payload[0])?,
            format: payload[1],
            size: u16::from_le_bytes([payload[2], payload[3]]),
        })
    }

    pub fn write(&self, out: &mut [u8]) {
        out[0] = self.kind.to_u8();
        out[1] = self.format;
        out[2..4].copy_from_slice(&self.size.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let orig = FontChunkHeader {
            kind: FontChunkKind::Grayscale,
            format: 4,
            size: 16,
        };
        let mut buf = [0u8; FONT_CHUNK_HEADER_LEN];
        orig.write(&mut buf);
        assert_eq!(FontChunkHeader::parse(&buf), Some(orig));
    }

    #[test]
    fn parse_rejects_short_payload() {
        assert_eq!(FontChunkHeader::parse(&[0, 0, 0]), None);
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        assert_eq!(FontChunkHeader::parse(&[9, 4, 16, 0]), None);
    }
}
