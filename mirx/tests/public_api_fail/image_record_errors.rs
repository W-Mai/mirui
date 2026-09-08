use mirx::image::{PlaneMemoryRecordError, SurfaceRecordError};

fn main() {
    let _ = PlaneMemoryRecordError::BufferTooSmall {
        needed: 24,
        available: 23,
    };
    let _ = SurfaceRecordError::BufferTooSmall {
        needed: 32,
        available: 31,
    };
}
