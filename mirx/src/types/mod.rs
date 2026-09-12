//! Shared validated value types.

pub mod color;
pub mod fixed;
pub mod fixed_64;
pub mod point;
pub mod rect;
pub mod transform;
pub mod transform_3d;

pub use crate::alignment::{ByteAlignment, InvalidByteAlignment};
pub use crate::media::MediaPayloadError as PayloadError;
pub use crate::media::integrity::DataIntegrity;
pub use color::Color;
pub use fixed::Fixed;
pub use fixed_64::Fixed64;
pub use point::Point;
pub use rect::Rect;
pub use transform::Transform;
pub use transform_3d::Transform3D;
