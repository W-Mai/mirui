use mirx::frames::{
    FrameCompositionError, FrameMapError, FrameTimingError, KeyframeIndexError,
};

fn main() {
    let _ = FrameCompositionError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = FrameMapError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = FrameTimingError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
    let _ = KeyframeIndexError::BufferTooSmall {
        needed: 1,
        available: 0,
    };
}
