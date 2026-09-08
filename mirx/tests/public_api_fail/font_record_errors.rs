use mirx::font::{CmapIndexError, FontFaceError, PlacementError};

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
}
