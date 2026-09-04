//! Bounded media sample codecs with caller-owned output.
//!
//! Sample streams contain no file header, geometry, color description, or CRC.
//! Media records and decode-unit boundaries supply those independent contracts.

mod pixel;
pub use pixel::{Pixel, PixelDecodePlan, PixelError};
