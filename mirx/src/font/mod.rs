mod asset;
mod codepoints;
mod face;
mod glyph;
mod mapping;
mod metrics;
mod owned;
mod record;
mod representation;
mod storage;
mod surface;
mod table;
mod view;

pub use asset::{FontAsset, GlyphSurfaceAsset, RepresentationAsset};
pub use owned::Font;

pub use codepoints::{FontCodepointError, FontCodepointIter, FontCodepoints};
pub use face::{FaceRepresentation, FaceTables, FaceTablesError};
pub use glyph::{Glyph, GlyphTable, GlyphTableError};
pub use mapping::{GLYPH_REGION_LEN, GlyphMap, GlyphMapError, GlyphPacking, GlyphRegions};
pub use metrics::{
    GLYPH_METRICS_LEN, GlyphMetrics, GlyphMetricsIter, LINE_METRICS_LEN, LineMetrics, MetricsError,
    MetricsTable,
};
pub use record::{REPRESENTATION_RECORD_LEN, RepresentationRecord, RepresentationRecordError};
pub use storage::{GlyphRaster, GlyphStorageError, RawGlyphBuilder, RawGlyphs};
pub use surface::{
    EncodedGlyphError, EncodedGlyphs, GLYPH_SURFACE_RECORD_LEN, GlyphGroups, GlyphSurfaceRecord,
    GlyphSurfaceRecordError,
};
pub use table::{RepresentationIter, RepresentationTable, RepresentationTableError};
pub use view::{FontError, FontGlyphs, FontView};

pub use crate::font::representation::{
    FontRepresentation, FontRepresentationError, FontRepresentationFallback,
    FontRepresentationKind, FontRepresentationMatch, FontRepresentationPreference,
    FontRepresentationRequest, FontRepresentations, FontSelectionError,
};
