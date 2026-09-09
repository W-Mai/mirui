//! Bounded Unicode layout and shaping shared by widget measurement and render backends.

pub mod layout;
pub mod mirx;
mod opentype;

pub use layout::{
    TextLayout, TextLayoutCache, TextLayoutError, TextLayoutHandle, TextLayoutLimits, TextMeasure,
};
pub use textflow::TextFlow;
pub use textflow::shaping::{
    FlowPoint, FontAccessError, FontFeature, FontId, FontMetrics, GlyphId, GlyphSource,
    PositionedGlyph, ShapeError, ShapeRequest, ShapedGlyph, SimpleTypeface, Typeface,
};
