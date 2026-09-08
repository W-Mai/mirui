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

pub use crate::error::FontAccessError;
pub use asset::{FontAdvanceSource, FontAsset, GlyphSurfaceAsset, RepresentationAsset};
pub use owned::Font;

pub(in crate::font) use header::FACE_RECORD_LEN;
pub use header::{FontFace, FontFaceError};
pub(in crate::font) use identity::{CMAP_INDEX_RECORD_LEN, GLYPH_ID_RECORD_LEN};
pub use identity::{CmapEntry, CmapIndex, CmapIndexError, GlyphId, GlyphIds, GlyphIdsError};
pub use mapping::{GlyphMap, GlyphMapError, GlyphPacking, GlyphRegions};
pub use metadata::{FontMetadata, FontMetadataError};
pub(in crate::font) use placement::{ADVANCE_RECORD_LEN, RASTER_METRICS_RECORD_LEN};
pub use placement::{Advances, PlacementError, RasterMetrics, RasterMetricsTable};
pub(in crate::font) use record::REPRESENTATION_RECORD_LEN;
pub use record::{RepresentationRecord, RepresentationRecordError};
pub use shaping::{ShapingData, ShapingDataError};
pub use storage::{GlyphRaster, GlyphStorageError, RawGlyphBuilder, RawGlyphs};
pub(in crate::font) use surface::GLYPH_SURFACE_RECORD_LEN;
pub use surface::{
    EncodedGlyphError, EncodedGlyphs, GlyphGroups, GlyphSurfaceRecord, GlyphSurfaceRecordError,
};
pub use table::{RepresentationIter, RepresentationTable, RepresentationTableError};
pub use view::{FontError, FontGlyphs, FontRepresentationView, FontView};

pub use crate::font::representation::{
    FontRepresentation, FontRepresentationError, FontRepresentationFallback,
    FontRepresentationKind, FontRepresentationMatch, FontRepresentationPreference,
    FontRepresentationRequest, FontRepresentations, FontSelectionError,
};
