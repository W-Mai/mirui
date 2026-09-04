//! Bounded media sample codecs with caller-owned output.
//!
//! Sample streams contain no file header, geometry, color description, or CRC.
//! Media records and decode-unit boundaries supply those independent contracts.

mod buffer;
mod frequency;
mod lz4;
mod pixel;
mod rle;
pub use frequency::{Frequency, FrequencyDecodePlan, FrequencyError, FrequencyGeometry};
pub use lz4::{Lz4, Lz4DecodePlan, Lz4Encoder, Lz4Error};
pub use pixel::{Pixel, PixelDecodePlan, PixelError};
pub use rle::{Rle, RleDecodePlan, RleError};
