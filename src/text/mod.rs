//! Bounded Unicode layout and shaping shared by widget measurement and render backends.

pub mod mirx;
mod opentype;

pub use textflow::TextFlow;
pub use textflow::shaping::{
    FlowPoint, FontAccessError, FontFeature, FontId, FontMetrics, GlyphId, GlyphSource, ShapeError,
    ShapeRequest, ShapedGlyph, SimpleTypeface, Typeface,
};
