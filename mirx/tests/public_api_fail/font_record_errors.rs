use mirx::font::{
    CmapIndexError, FontFaceError, GlyphSurfaceRecordError, PlacementError,
    RepresentationRecordError,
};

fn main() {
    let _ = FontFaceError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = CmapIndexError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = PlacementError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = RepresentationRecordError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = GlyphSurfaceRecordError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
}
