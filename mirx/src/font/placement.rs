use crate::types::Fixed;

pub(in crate::font) const ADVANCE_RECORD_LEN: usize = 4;
pub(in crate::font) const RASTER_METRICS_RECORD_LEN: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Advances<'a> {
    bytes: &'a [u8],
}

impl<'a> Advances<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, PlacementError> {
        if bytes.len() % ADVANCE_RECORD_LEN != 0 {
            return Err(PlacementError::PartialAdvance {
                byte_len: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / ADVANCE_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, ordinal: usize) -> Option<Fixed> {
        let offset = ordinal.checked_mul(ADVANCE_RECORD_LEN)?;
        let bytes = self.bytes.get(offset..offset + ADVANCE_RECORD_LEN)?;
        Some(Fixed::from_le_bytes(bytes.try_into().ok()?))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RasterMetrics {
    offset_x: Fixed,
    offset_y: Fixed,
}

impl RasterMetrics {
    pub const fn new(offset_x: Fixed, offset_y: Fixed) -> Self {
        Self { offset_x, offset_y }
    }

    pub const fn offset_x(self) -> Fixed {
        self.offset_x
    }

    pub const fn offset_y(self) -> Fixed {
        self.offset_y
    }

    pub(in crate::font) fn encode_record(self) -> [u8; RASTER_METRICS_RECORD_LEN] {
        let mut record = [0; RASTER_METRICS_RECORD_LEN];
        record[..4].copy_from_slice(&self.offset_x.to_le_bytes());
        record[4..].copy_from_slice(&self.offset_y.to_le_bytes());
        record
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterMetricsTable<'a> {
    bytes: &'a [u8],
}

impl<'a> RasterMetricsTable<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, PlacementError> {
        if bytes.len() % RASTER_METRICS_RECORD_LEN != 0 {
            return Err(PlacementError::PartialRasterMetrics {
                byte_len: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / RASTER_METRICS_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, ordinal: usize) -> Option<RasterMetrics> {
        let offset = ordinal.checked_mul(RASTER_METRICS_RECORD_LEN)?;
        let bytes = self.bytes.get(offset..offset + RASTER_METRICS_RECORD_LEN)?;
        Some(RasterMetrics {
            offset_x: Fixed::from_le_bytes(bytes[..4].try_into().ok()?),
            offset_y: Fixed::from_le_bytes(bytes[4..].try_into().ok()?),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PlacementError {
    PartialAdvance { byte_len: usize },
    PartialRasterMetrics { byte_len: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_and_raster_offsets_preserve_signed_q24_8_values() {
        let advances = Advances::open(&[0, 1, 0, 0, 0, 254, 255, 255]).unwrap();
        assert_eq!(advances.get(0), Some(Fixed::ONE));
        assert_eq!(
            advances.get(1).unwrap().to_le_bytes(),
            (-512_i32).to_le_bytes()
        );

        let mut bytes = [0; RASTER_METRICS_RECORD_LEN];
        bytes[..4].copy_from_slice(&(-384_i32).to_le_bytes());
        bytes[4..].copy_from_slice(&640_i32.to_le_bytes());
        let metric = RasterMetricsTable::open(&bytes).unwrap().get(0).unwrap();
        assert_eq!(metric.offset_x().to_le_bytes(), (-384_i32).to_le_bytes());
        assert_eq!(metric.offset_y().to_le_bytes(), 640_i32.to_le_bytes());

        assert_eq!(metric.encode_record(), bytes);
    }

    #[test]
    fn placement_tables_reject_partial_records() {
        assert_eq!(
            Advances::open(&[0; 3]),
            Err(PlacementError::PartialAdvance { byte_len: 3 })
        );
        assert_eq!(
            RasterMetricsTable::open(&[0; 7]),
            Err(PlacementError::PartialRasterMetrics { byte_len: 7 })
        );
    }
}
