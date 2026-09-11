//! Bounded Unicode layout and shaping shared by widget measurement and render backends.

pub(crate) mod layout;
pub mod mirx;
mod opentype;
pub(crate) mod path;

pub use layout::TextLayoutLimits;
pub(crate) use layout::{TextLayout, TextLayoutHandle, TextMeasure};
pub use path::{PathAccess, PathAccessError, PathDirection, TextPath};
pub use textflow::TextFlow;
pub use textflow::shaping::{
    FlowPoint, FontAccessError, FontFeature, FontId, FontMetrics, GlyphId, GlyphSource,
    PositionedGlyph, ShapeError, ShapeRequest, ShapedGlyph, SimpleTypeface, Typeface,
};
