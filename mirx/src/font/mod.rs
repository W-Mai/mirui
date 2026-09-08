mod asset;
mod header;
mod identity;
mod mapping;
mod metadata;
mod owned;
mod placement;
mod record;
mod representation;
mod shaping;
mod storage;
mod surface;
mod table;
mod view;

pub use asset::{FontAdvanceSource, FontAsset, GlyphSurfaceAsset, RepresentationAsset};
pub use owned::Font;

pub use header::{FACE_RECORD_LEN, FontFace, FontFaceError};
pub use identity::{
    CMAP_INDEX_RECORD_LEN, CmapEntry, CmapIndex, CmapIndexError, GLYPH_ID_RECORD_LEN, GlyphId,
    GlyphIds, GlyphIdsError,
};
pub use mapping::{GLYPH_REGION_LEN, GlyphMap, GlyphMapError, GlyphPacking, GlyphRegions};
pub use metadata::{FontMetadata, FontMetadataError};
pub use placement::{
    ADVANCE_RECORD_LEN, Advances, PlacementError, RASTER_METRICS_RECORD_LEN, RasterMetrics,
    RasterMetricsTable,
};
pub use record::{REPRESENTATION_RECORD_LEN, RepresentationRecord, RepresentationRecordError};
pub use shaping::{SFNT_HEADER_LEN, SFNT_TABLE_RECORD_LEN, ShapingData, ShapingDataError};
pub use storage::{GlyphRaster, GlyphStorageError, RawGlyphBuilder, RawGlyphs};
pub use surface::{
    EncodedGlyphError, EncodedGlyphs, GLYPH_SURFACE_RECORD_LEN, GlyphGroups, GlyphSurfaceRecord,
    GlyphSurfaceRecordError,
};
pub use table::{RepresentationIter, RepresentationTable, RepresentationTableError};
pub use view::{FontError, FontGlyphs, FontRepresentationView, FontView};

pub use crate::font::representation::{
    FontRepresentation, FontRepresentationError, FontRepresentationFallback,
    FontRepresentationKind, FontRepresentationMatch, FontRepresentationPreference,
    FontRepresentationRequest, FontRepresentations, FontSelectionError,
};
