use crate::{
    types::Fixed,
    wire::{read_u16_le, write_u16_le},
};

pub(in crate::font) const FACE_RECORD_LEN: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontFace {
    units_per_em: u16,
    default_glyph: super::GlyphId,
    raster_count: u16,
    ascender: Fixed,
    descender: Fixed,
    line_gap: Fixed,
}

impl FontFace {
    pub const fn new(
        units_per_em: u16,
        default_glyph: super::GlyphId,
        raster_count: u16,
        ascender: Fixed,
        descender: Fixed,
        line_gap: Fixed,
    ) -> Result<Self, FontFaceError> {
        if units_per_em == 0 {
            return Err(FontFaceError::ZeroUnitsPerEm);
        }
        if raster_count == 0 {
            return Err(FontFaceError::ZeroRasterCount);
        }
        Ok(Self {
            units_per_em,
            default_glyph,
            raster_count,
            ascender,
            descender,
            line_gap,
        })
    }

    pub(in crate::font) fn from_record(bytes: &[u8]) -> Result<Self, FontFaceError> {
        if bytes.len() < FACE_RECORD_LEN {
            return Err(FontFaceError::Truncated {
                needed: FACE_RECORD_LEN,
                available: bytes.len(),
            });
        }
        if bytes.len() != FACE_RECORD_LEN {
            return Err(FontFaceError::TrailingBytes {
                byte_len: bytes.len(),
            });
        }
        let units_per_em = read_u16_le(bytes, 0).expect("complete face record");
        if units_per_em == 0 {
            return Err(FontFaceError::ZeroUnitsPerEm);
        }
        let raster_count = read_u16_le(bytes, 4).expect("complete face record");
        if raster_count == 0 {
            return Err(FontFaceError::ZeroRasterCount);
        }
        if read_u16_le(bytes, 6).expect("complete face record") != 0 {
            return Err(FontFaceError::ReservedNonZero);
        }
        Self::new(
            units_per_em,
            super::GlyphId::new(read_u16_le(bytes, 2).expect("complete face record")),
            raster_count,
            Self::fixed(bytes, 8),
            Self::fixed(bytes, 12),
            Self::fixed(bytes, 16),
        )
    }

    pub(in crate::font) fn encode_record_into(
        self,
        output: &mut [u8],
    ) -> Result<usize, FontFaceError> {
        if output.len() < FACE_RECORD_LEN {
            return Err(FontFaceError::BufferTooSmall {
                needed: FACE_RECORD_LEN,
                available: output.len(),
            });
        }
        let mut record = [0; FACE_RECORD_LEN];
        write_u16_le(&mut record, 0, self.units_per_em);
        write_u16_le(&mut record, 2, self.default_glyph.get());
        write_u16_le(&mut record, 4, self.raster_count);
        record[8..12].copy_from_slice(&self.ascender.to_le_bytes());
        record[12..16].copy_from_slice(&self.descender.to_le_bytes());
        record[16..20].copy_from_slice(&self.line_gap.to_le_bytes());
        output[..FACE_RECORD_LEN].copy_from_slice(&record);
        Ok(FACE_RECORD_LEN)
    }

    pub const fn units_per_em(self) -> u16 {
        self.units_per_em
    }

    pub const fn default_glyph(self) -> super::GlyphId {
        self.default_glyph
    }

    pub const fn raster_count(self) -> u16 {
        self.raster_count
    }

    pub const fn ascender(self) -> Fixed {
        self.ascender
    }

    pub const fn descender(self) -> Fixed {
        self.descender
    }

    pub const fn line_gap(self) -> Fixed {
        self.line_gap
    }

    fn fixed(bytes: &[u8], offset: usize) -> Fixed {
        Fixed::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("complete face record"),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontFaceError {
    Truncated { needed: usize, available: usize },
    BufferTooSmall { needed: usize, available: usize },
    TrailingBytes { byte_len: usize },
    ZeroUnitsPerEm,
    ZeroRasterCount,
    ReservedNonZero,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> [u8; FACE_RECORD_LEN] {
        let mut bytes = [0; FACE_RECORD_LEN];
        bytes[0..2].copy_from_slice(&1_000_u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&7_u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&23_u16.to_le_bytes());
        bytes[8..12].copy_from_slice(&800_i32.to_le_bytes());
        bytes[12..16].copy_from_slice(&(-200_i32).to_le_bytes());
        bytes[16..20].copy_from_slice(&100_i32.to_le_bytes());
        bytes
    }

    #[test]
    fn face_record_preserves_identity_counts_and_signed_metrics() {
        let bytes = face();
        let value = FontFace::from_record(&bytes).unwrap();
        assert_eq!(value.units_per_em(), 1_000);
        assert_eq!(value.default_glyph(), super::super::GlyphId::new(7));
        assert_eq!(value.raster_count(), 23);
        assert_eq!(value.ascender().to_le_bytes(), 800_i32.to_le_bytes());
        assert_eq!(value.descender().to_le_bytes(), (-200_i32).to_le_bytes());
        assert_eq!(value.line_gap().to_le_bytes(), 100_i32.to_le_bytes());

        let mut output = [0x5a; FACE_RECORD_LEN + 1];
        assert_eq!(value.encode_record_into(&mut output), Ok(FACE_RECORD_LEN));
        assert_eq!(output[..FACE_RECORD_LEN], bytes);
        assert_eq!(output[FACE_RECORD_LEN], 0x5a);
    }

    #[test]
    fn face_record_rejects_noncanonical_lengths_and_zero_fields() {
        assert!(matches!(
            FontFace::from_record(&face()[..19]),
            Err(FontFaceError::Truncated { .. })
        ));
        let mut trailing = [0; FACE_RECORD_LEN + 1];
        trailing[..FACE_RECORD_LEN].copy_from_slice(&face());
        assert_eq!(
            FontFace::from_record(&trailing),
            Err(FontFaceError::TrailingBytes { byte_len: 21 })
        );
        let mut bytes = face();
        bytes[0..2].fill(0);
        assert_eq!(
            FontFace::from_record(&bytes),
            Err(FontFaceError::ZeroUnitsPerEm)
        );
        let mut bytes = face();
        bytes[4..6].fill(0);
        assert_eq!(
            FontFace::from_record(&bytes),
            Err(FontFaceError::ZeroRasterCount)
        );
        let mut bytes = face();
        bytes[6] = 1;
        assert_eq!(
            FontFace::from_record(&bytes),
            Err(FontFaceError::ReservedNonZero)
        );
    }
}
