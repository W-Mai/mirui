//! Geometry primitives used by the VECTOR chunk wire format.
//!
//! All types in this module are plain data — `pub` fields, no
//! arithmetic, no methods beyond constructors and accessors. Consumers
//! convert to their own runtime types via `From` and do math there.

pub mod color;
pub mod fixed;
pub mod point;
pub mod rect;
pub mod transform;

pub use color::Color;
pub use fixed::Fixed;
pub use point::Point;
pub use rect::Rect;
pub use transform::Transform;
