mod codec;
mod header;
mod op;
mod paint;

pub use crate::error::VectorAccessError;
pub use crate::path::{Path, PathCmd};
pub use codec::{CodecError, VectorEncodeError, VectorReadError};
pub(crate) use header::VectorChunkHeader;
pub use op::{
    CompositeMode, FillRule, GlyphPlacement, GlyphPose, LineCap, LineJoin, ResourceRef, Scene,
    SceneOp,
};
pub use paint::{GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode};
