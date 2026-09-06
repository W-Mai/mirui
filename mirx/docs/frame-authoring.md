# Sectioned frame authoring

`FramesEncoder` converts decoded tight frames into one canonical sectioned `FRAMES` payload. The source surface remains the only owner of width, height, sample layout, color description, and derived plane geometry.

`FramesPlaybackPlan::access_capabilities()` reports complete-frame playback and bounded random-frame seeking only after every frame path has passed preflight. Sparse updates and previous-frame residuals are internal storage choices; they do not advertise arbitrary image-region playback, progressive delivery, or direct GPU upload.

Each `push` compares complete representations. RAW is the independent fallback; RLE, native pixel coding, LZ4, and reversible frequency coding are independent lossless keyframe candidates; `FrameDelta` predicts from the previous reconstructed frame. Quantized frequency coding participates only when configured through `FrameEncodingSet::with_quantized_frequency` and admitted through `FramePolicy::allow_lossy`. An unchanged frame can omit groups and DATA entirely. Changed tiles also compete through every enabled profile. The selector compares group records, selection and range indexes, new coding-table entries, DATA alignment padding, and encoded bytes rather than comparing codec bodies alone.

```rust
use mirx::{
    ByteAlignment, FrameSequence, FramesEncoder,
    image::{ColorDescription, SampleLayout, SurfaceDescriptor},
};

let surface = SurfaceDescriptor::new(
    2,
    1,
    SampleLayout::RGBA8888,
    ColorDescription::SRGB,
)
.unwrap();
let sequence = FrameSequence::new(2, 1_000, 40)
    .unwrap()
    .with_max_delta_frames(1)
    .unwrap();
let mut encoder = FramesEncoder::new(sequence, surface)
    .unwrap()
    .with_tiles(16, 16)
    .unwrap()
    .with_input_alignment(ByteAlignment::new(64).unwrap())
    .unwrap();
encoder
    .push(&[255, 0, 0, 255, 0, 0, 0, 255])
    .unwrap();
encoder
    .push_with_duration(&[254, 0, 0, 255, 0, 0, 0, 255], 75)
    .unwrap();
let encoded = encoder.finish().unwrap();
let payload: &[u8] = encoded.payload();
# assert!(!payload.is_empty());
```

`Document::push_frames(encoded)` transfers the finished payload without cloning it. The document writer reads its declared group input alignment and chooses a chunk position that makes the final file-relative DATA addresses valid. `Document::frames(id)` and `ChunkRef::frames(limits)` return the same `FramesView`; both retain fixed file placement when the payload came from an encoded container.

`SurfaceDescriptor::tight_byte_len` derives the required input length by summing every plane's `minimum_stride() × height`; packed indexes and subsampled YUV therefore use the same geometry seam as decoding. `with_tiles` validates one joint-plane grid, including chroma boundaries, before the first frame. Sparse selection chooses the smaller sorted-cell or checkpointed-bitmap form. Equal coded unit lengths omit the range table; variable lengths use compact checkpointed `u16` lengths when possible and `u32` otherwise. `with_input_alignment` aligns the DATA base and every stored unit start. Runtime output alignment and stride remain separate `SurfaceRequirements`, so a 64-byte GPU input requirement does not silently force the same decoded layout.

Indexed surfaces require `with_color_table` before the first frame. The exact RGBA table is stored once for the sequence; sparse tiles carry only packed index samples. `without_tiles` disables spatial candidates when whole-frame coding is preferred.

`FrameWriteReport` exposes the selected storage relation, coding, encoded body size, complete incremental storage size, and resulting recovery distance for every frame. Failed sizing, candidate selection, or storage reservation does not advance the sequence.

`push` uses `FrameSequence::default_duration_ticks`; `push_with_duration` accepts a nonzero duration in the same sequence timebase. Canonical authoring omits the timing section when every frame uses the default and otherwise chooses the smaller dense or sparse override form.

After a quantized keyframe or sparse tile group is selected, the encoder reconstructs that exact coded output into its retained history. Later omission and `FrameDelta` candidates therefore use the same predictor bytes as runtime playback. This prevents source samples that were discarded by quantization from leaking into the inter-frame reference chain.
