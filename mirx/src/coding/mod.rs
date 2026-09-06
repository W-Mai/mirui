//! Bounded media sample codecs with caller-owned output.
//!
//! Sample streams contain no file header, geometry, color description, or CRC.
//! Media records and decode-unit boundaries supply those independent contracts.

mod buffer;
mod frame_delta;
mod frequency;
mod lz4;
mod pixel;
mod rle;
#[cfg(target_arch = "aarch64")]
pub use frame_delta::NeonFrameDelta;
#[cfg(target_arch = "x86_64")]
pub use frame_delta::Sse2FrameDelta;
pub use frame_delta::{
    FrameDelta, FrameDeltaDecodePlan, FrameDeltaError, FrameDeltaKernel, ScalarFrameDelta,
};
pub use frequency::{Frequency, FrequencyDecodePlan, FrequencyError, FrequencyGeometry};
pub use lz4::{Lz4, Lz4DecodePlan, Lz4Encoder, Lz4Error};
pub use pixel::{Pixel, PixelDecodePlan, PixelError};
pub use rle::{Rle, RleDecodePlan, RleError};
