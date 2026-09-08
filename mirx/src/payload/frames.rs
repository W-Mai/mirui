#[path = "frames/composition.rs"]
mod composition;
#[doc = include_str!("../../docs/frame-authoring.md")]
#[path = "frames/encode.rs"]
mod encode;
#[path = "frames/frame_map.rs"]
mod frame_map;
#[path = "frames/keyframes.rs"]
mod keyframes;
#[path = "frames/playback.rs"]
mod playback;
#[path = "frames/sectioned.rs"]
mod sectioned;
#[doc = include_str!("../../docs/frame-selection.md")]
#[path = "frames/selection.rs"]
mod selection;
#[path = "frames/sequence.rs"]
mod sequence;
#[path = "frames/timeline.rs"]
mod timeline;
#[path = "frames/timing.rs"]
mod timing;

pub use crate::error::FramesAccessError;
pub use composition::{
    FrameComposition, FrameCompositionError, FrameCompositionOverride, FrameCompositionTable,
};
pub use encode::{
    EncodedFrames, FrameEncoding, FrameEncodingSet, FrameWriteError, FrameWriteReport,
    FramesEncoder,
};
pub use frame_map::{FrameCounts, FrameMap, FrameMapError, FrameMapIter};
pub use keyframes::{KeyframeIndex, KeyframeIndexError, KeyframeIter};
pub use playback::{
    FrameDecodeError, FrameDecodePlan, FrameDisposalPlan, FrameSession, FramesPlaybackPlan,
    PlaybackStorage,
};
pub use sectioned::{
    FrameGroupIter, FrameGroups, FramePresentation, FramesAsset, FramesEncodeError, FramesError,
    FramesView,
};
pub use selection::{
    CandidateRejection, FrameCandidate, FrameChoice, FramePolicy, FrameSelectionError,
    FrameSelector, FrameStorage,
};
pub use sequence::{BlendMode, DisposalMode, FrameSequence, FrameSequenceError};
pub use timeline::{FramePosition, FrameTimeline};
pub use timing::{FrameTiming, FrameTimingEncoding, FrameTimingError};
