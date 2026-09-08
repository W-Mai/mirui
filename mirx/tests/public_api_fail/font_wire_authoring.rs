use mirx::font::{
    ADVANCE_RECORD_LEN, CMAP_INDEX_RECORD_LEN, FACE_RECORD_LEN, GLYPH_ID_RECORD_LEN,
    GLYPH_SURFACE_RECORD_LEN, CmapEntry, FontFace, GlyphPacking, GlyphSurfaceRecord,
    RASTER_METRICS_RECORD_LEN, REPRESENTATION_RECORD_LEN, RasterMetrics, RepresentationRecord,
    SFNT_HEADER_LEN, SFNT_TABLE_RECORD_LEN,
};
use mirx::image::SampleLayout;

fn main() {
    let _ = RepresentationRecord::new;
    let _ = RepresentationRecord::from_record;
    let _ = RepresentationRecord::encode_record_into;
    let _ = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 0);
    let _ = GlyphSurfaceRecord::from_record;
    let _ = GlyphSurfaceRecord::encode_record_into;
    let _ = FontFace::from_record;
    let _ = FontFace::encode_record;
    let _ = CmapEntry::encode_record;
    let _ = RasterMetrics::encode_record;
    let _ = (
        ADVANCE_RECORD_LEN,
        CMAP_INDEX_RECORD_LEN,
        FACE_RECORD_LEN,
        GLYPH_ID_RECORD_LEN,
        GLYPH_SURFACE_RECORD_LEN,
        RASTER_METRICS_RECORD_LEN,
        REPRESENTATION_RECORD_LEN,
        SFNT_HEADER_LEN,
        SFNT_TABLE_RECORD_LEN,
    );
}
