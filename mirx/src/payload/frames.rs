mod composition;
#[doc = include_str!("../../docs/frame-authoring.md")]
mod encode;
mod frame_map;
mod keyframes;
mod playback;
mod sectioned;
#[doc = include_str!("../../docs/frame-selection.md")]
mod selection;
mod sequence;
mod timeline;
mod timing;

pub use composition::{
    FRAME_COMPOSITION_RECORD_LEN, FrameComposition, FrameCompositionAsset, FrameCompositionError,
    FrameCompositionOverride, FrameCompositionTable,
};
pub use encode::{
    EncodedFrames, FrameEncoding, FrameEncodingSet, FrameWriteError, FrameWriteReport,
    FramesEncoder,
};
pub use frame_map::{FrameCounts, FrameMap, FrameMapAsset, FrameMapError, FrameMapIter};
pub use keyframes::{KeyframeIndex, KeyframeIndexAsset, KeyframeIndexError, KeyframeIter};
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
pub use sequence::{
    BlendMode, DisposalMode, FRAME_SEQUENCE_RECORD_LEN, FrameSequence, FrameSequenceError,
};
pub use timeline::{FramePosition, FrameTimeline};
pub use timing::{
    FRAME_TIMING_HEADER_LEN, FrameTiming, FrameTimingAsset, FrameTimingEncoding, FrameTimingError,
};
