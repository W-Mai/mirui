# Encoded images

`EncodedImageAsset` wraps one already encoded stream with a surface descriptor and coding record. Metadata authoring, integrity checking and sample decoding are separate operations. `encoded_len`, `encode_into` and `matches_payload` allocate nothing; `encode` allocates one payload.

## Encode, store and decode

```rust
use mirx::coding::Rle;
use mirx::image::{
    ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageView,
    SampleLayout, SurfaceDescriptor, SurfaceRequirements,
};

let surface = SurfaceDescriptor::new(
    4, 2, SampleLayout::A8, ColorDescription::NONE,
).unwrap();
let codec = Rle::new();
let mut stream = [0; 16];
let stream_len = codec.encode_into(&[42; 8], &mut stream).unwrap();
let asset = EncodedImageAsset::new(surface, codec.record(), &stream[..stream_len]);
let mut payload = [0; 128];
let payload_len = asset.encode_into(&mut payload).unwrap();

let image = EncodedImageView::open(&payload[..payload_len]).unwrap();
let mut scratch = [None];
let groups = image.groups_into(&mut scratch, &mut CoverageBudget::new(100)).unwrap();
groups.validate_unit(0, 0).unwrap();
let unit = groups.get(0).unwrap().get(0).unwrap();
let plan = unit.decode_plan(SurfaceRequirements::new().with_stride_multiple(64)).unwrap();
let mut output = [0; 128];
let decoded = plan.decode_into(&mut output).unwrap();
assert_eq!(decoded.plane(0).unwrap().row(1).unwrap(), Some(&[42; 4][..]));
```

The encoder receives tight logical sample bytes, not physical row padding. PIXEL accepts RGB888/RGBA8888 samples; RLE and LZ4 operate on bytes. For a joint YUV stream, concatenate tight rows in plane-index order. Indexed color tables are separate metadata, supplied with `with_color_table`; they are not compressed with the index plane. No color conversion or automatic codec selection occurs.

## Payload layout

Default single-stream storage has 92 bytes of overhead, excluding coding parameters and DATA:

| Byte range | Content | Responsibility |
| --- | --- | --- |
| 0–7 | Media header | Version, flags, section count, metadata CRC |
| 8–43 | 3 section entries | Offsets and sizes for SURFACE, CODINGS and DATA |
| 44–75 | SURFACE | Logical extent, sample layout and color description |
| 76–87 | CODINGS | Count 1, coding ID, revision, parameter end |
| 88 onward | Parameters, then DATA | Profile parameters and supplied stream |
| Final 4 bytes | DATA CRC | Integrity of the complete stream |

UNIT_GROUPS and UNIT_INDEX are omitted: the only coding and complete DATA range define one whole-surface unit. Empty surfaces require empty DATA and resolve to no units. RAW surfaces use `RawImageAsset` and omit CODINGS. A color table adds one 12-byte directory entry and its RGBA bytes.

`with_input_alignment(A)` requires a power of two. For a nonempty image with `A > 1`, authoring adds one 36-byte group record and directory entry, then zero-pads metadata so the DATA offset is a multiple of A. Empty images omit meaningless group/alignment records. A `Vec<u8>` or an arbitrary output slice does not acquire a pointer-alignment guarantee from serialization.

## Validation boundaries

| Operation | Checks |
| --- | --- |
| `asset.encoded_len` / `encode_into` | Surface and palette agreement, single-unit geometry, lengths, placement and output capacity |
| `EncodedImageView::open` | Metadata CRC, sections and typed metadata; no DATA scan |
| `image.groups_into` | Group geometry, static coverage, indexes and declared file alignment |
| `groups.validate_unit` | Declared checksum coverage, reported as bytes read |
| `unit.decode_plan` | Supported profile syntax, exact decoded length and target memory requirements |
| `plan.decode_into` | Actual target address/capacity, then reconstruction into caller storage |

Unknown nonzero coding IDs, revisions and parameters remain representable. Even known-profile bytes are not decoded during authoring; malformed streams can be preserved but fail explicit decode preflight. Checksums prove byte integrity, not valid coding syntax.

Errors from `encode_into` preserve the entire output; success preserves its unused suffix. Canonical comparison includes directory entries, padding and both checksums. `image::ImageEncodeError` is shared by RAW and encoded surface authoring. The packed `ImageAsset` facade retains its payload-validation error wrapper.

The asset writer accepts one stream, not a list of separately encoded tiles. It does not integrate compressed storage into `ImageSource`, `Document::push_image` or runtime rendering. Unit-index APIs and group readers describe grouped storage independently.
