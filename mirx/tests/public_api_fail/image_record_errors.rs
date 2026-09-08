use mirx::image::{
    EncodedGroupError, PlaneMemoryRecordError, SurfaceRecordError, UnitIndexError,
    UnitSelectionError,
};

fn main() {
    let _ = PlaneMemoryRecordError::BufferTooSmall {
        needed: 24,
        available: 23,
    };
    let _ = SurfaceRecordError::BufferTooSmall {
        needed: 32,
        available: 31,
    };
    let _ = EncodedGroupError::BufferTooSmall {
        needed: 36,
        available: 35,
    };
    let _ = UnitIndexError::BufferTooSmall {
        needed: 4,
        available: 3,
    };
    let _ = UnitSelectionError::BufferTooSmall {
        needed: 4,
        available: 3,
    };
}
