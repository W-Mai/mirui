# Sectioned frame authoring

`FramesEncoder` converts decoded tight frames into one canonical sectioned `FRAMES` payload. The source surface remains the only owner of width, height, sample layout, color description, and derived plane geometry.

Each `push` compares complete lossless representations. RAW is the independent fallback; RLE, native pixel coding, and LZ4 are independent keyframe candidates; `FrameDelta` predicts from the previous reconstructed frame. An unchanged frame can omit groups and DATA entirely. The selector compares group records, new coding-table entries, DATA alignment padding, and encoded bytes rather than comparing codec bodies alone.

```rust
use mirx::{
    FrameSequence, FramesEncoder,
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
    .with_input_alignment(64)
    .unwrap();
encoder
    .push(&[255, 0, 0, 255, 0, 0, 0, 255])
    .unwrap();
encoder
    .push(&[254, 0, 0, 255, 0, 0, 0, 255])
    .unwrap();
let encoded = encoder.finish().unwrap();
let payload: &[u8] = encoded.payload();
# assert!(!payload.is_empty());
```

`SurfaceDescriptor::tight_byte_len` derives the required input length by summing every plane's `minimum_stride() × height`; packed indexes and subsampled YUV therefore use the same geometry seam as decoding. `with_input_alignment` aligns the DATA base and every stored unit start. Runtime output alignment and stride remain separate `SurfaceRequirements`, so a 64-byte GPU input requirement does not silently force the same decoded layout.

`FrameWriteReport` exposes the selected storage relation, coding, encoded body size, complete incremental storage size, and resulting recovery distance for every frame. Failed sizing, candidate selection, or storage reservation does not advance the sequence.
