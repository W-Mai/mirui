use mirx::frames::{
    FRAME_COMPOSITION_RECORD_LEN, FRAME_SEQUENCE_RECORD_LEN, FRAME_TIMING_HEADER_LEN,
    FrameCompositionAsset, FrameCounts, FrameMap, FrameMapAsset, FrameSequence,
    FrameTimingAsset, KeyframeIndexAsset,
};

fn main() {
    let _ = FrameCompositionAsset::new;
    let _ = FrameMapAsset::new;
    let _ = FrameTimingAsset::new;
    let _ = KeyframeIndexAsset::new;
    let _ = FrameSequence::open;
    let _ = FrameSequence::encode_record;
    let _ = FrameCounts::encoded_len;
    let _ = FrameCounts::encode_into;
    let _ = FrameMap::encoded_len;
    let _ = (
        FRAME_COMPOSITION_RECORD_LEN,
        FRAME_SEQUENCE_RECORD_LEN,
        FRAME_TIMING_HEADER_LEN,
    );
}
