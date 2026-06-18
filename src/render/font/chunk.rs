//! Self-describing prefix on every FONT chunk payload.
//!
//! A FONT payload starts with a [`FontChunkHeader`] so a reader knows
//! the rasterization scheme and source size before parsing the
//! format-specific body. The bytes after the header are the SDF
//! [`AtlasHeader`](super::sdf::AtlasHeader), decided by `kind`.

/// Length of the shared prefix every FONT payload starts with.
pub const FONT_CHUNK_HEADER_LEN: usize = 4;

/// Rasterization scheme stored in a FONT chunk, parallel to
/// [`super::GlyphKind`]. Serialized as the first byte of a FONT payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontChunkKind {
    Sdf,
}

impl FontChunkKind {
    fn from_u8(b: u8) -> Option<Self> {
        match b {
            1 => Some(FontChunkKind::Sdf),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            FontChunkKind::Sdf => 1,
        }
    }
}

/// Shared 4-byte prefix on every FONT chunk payload.
///
/// For SDF (which scales one atlas to any target) `size` carries the
/// source size the atlas was baked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontChunkHeader {
    pub kind: FontChunkKind,
    /// bit_depth for SDF.
    pub format: u8,
    pub size: u16,
}

impl FontChunkHeader {
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < FONT_CHUNK_HEADER_LEN {
            return None;
        }
        Some(FontChunkHeader {
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
            kind: FontChunkKind::Sdf,
            format: 4,
            size: 32,
        };
        let mut buf = [0u8; FONT_CHUNK_HEADER_LEN];
        orig.write(&mut buf);
        assert_eq!(FontChunkHeader::parse(&buf), Some(orig));
    }

    #[test]
    fn parse_rejects_short_payload() {
        assert_eq!(FontChunkHeader::parse(&[1, 0, 0]), None);
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        assert_eq!(FontChunkHeader::parse(&[9, 4, 32, 0]), None);
    }
}
