//! Shared validated value types.

pub mod color;
pub mod fixed;
pub mod point;
pub mod rect;
pub mod transform;

pub use crate::alignment::{ByteAlignment, InvalidByteAlignment};
pub use crate::media::integrity::DataIntegrity;
pub use color::Color;
pub use fixed::Fixed;
pub use point::Point;
pub use rect::Rect;
pub use transform::Transform;
