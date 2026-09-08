//! Bounded media sample codecs with caller-owned output.
//!
//! Sample streams contain no file header, geometry, color description, or CRC.
//! Media records and decode-unit boundaries supply those independent contracts.

/// Open identifier for one stored coding profile.
///
/// Unknown values remain representable for opaque payload preservation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodingId(u16);

impl CodingId {
    pub const RAW: Self = Self(0);
    pub const PIXEL: Self = Self(1);
    pub const RLE: Self = Self(2);
    pub const LZ4: Self = Self(3);
    pub const FREQUENCY_REVERSIBLE: Self = Self(4);
    pub const FREQUENCY_QUANTIZED: Self = Self(5);
    pub const FRAME_DELTA: Self = Self(6);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl From<u16> for CodingId {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

impl From<CodingId> for u16 {
    fn from(value: CodingId) -> Self {
        value.raw()
    }
}

mod buffer;
mod frame_delta;
mod frequency;
mod lz4;
mod pixel;
mod rle;
pub use crate::media::coding::{CodingRecord, CodingTable, CodingTableError};
pub use frame_delta::{
    FrameDelta, FrameDeltaDecodePlan, FrameDeltaError, FrameDeltaKernel, ScalarFrameDelta,
};
pub use frequency::{Frequency, FrequencyDecodePlan, FrequencyError, FrequencyGeometry};
pub use lz4::{Lz4, Lz4DecodePlan, Lz4Encoder, Lz4Error};
pub use pixel::{Pixel, PixelDecodePlan, PixelError};
pub use rle::{Rle, RleDecodePlan, RleError};

#[cfg(test)]
mod id_tests {
    use super::CodingId;

    #[test]
    fn retains_unknown_values() {
        assert_eq!(CodingId::RAW.raw(), 0);
        assert_eq!(CodingId::new(0xbeef).raw(), 0xbeef);
    }
}
