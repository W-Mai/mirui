//! Bounded Unicode layout and shaping shared by widget measurement and render backends.

pub(crate) mod baseline;
pub(crate) mod geometry;
pub(crate) mod layout;
pub mod mirx;
mod opentype;
pub(crate) mod path;

pub use crate::ui::widgets::text::{
    PathCaretGeometry, PathCaretHit, PathSelectionRibbon, PathTextGeometryError,
};
pub use geometry::{TextGeometry, TextGeometryRead};
pub use layout::TextLayout;
pub use layout::{TextLayoutBuffer, TextLayoutCapacity, TextLayoutError, TextLayoutLimits};
pub(crate) use layout::{TextLayoutHandle, TextMeasure};
pub use path::{PathAccess, PathAccessError, PathDirection, TextPath};
pub use textflow::TextFlow;
pub use textflow::shaping::{
    FlowPoint, FontAccessError, FontFeature, FontId, FontMetrics, GlyphId, GlyphSource,
    PositionedGlyph, ShapeError, ShapeRequest, ShapedGlyph, SimpleTypeface, Typeface,
};
pub use textflow::workspace::WorkspaceCapacity;
