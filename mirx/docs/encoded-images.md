# Encoded images

`EncodedImageAsset` wraps already encoded DATA with a surface descriptor, coding records and optional groups/indexes. Metadata authoring, integrity checking and sample decoding are separate operations. `encoded_len`, `encode_into` and `matches_payload` allocate nothing; `encode` allocates one payload.

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

`image::ImageRef::open(payload)` and `open_at(payload, file_offset)` inspect either storage form. Match `ImageRef::Raw(surface)` for direct verified samples or `ImageRef::Encoded(image)` for explicit group/decode work. `surface()` and `color_table()` expose common metadata; `raw()` and `encoded()` are optional typed projections, not conversion requests.

The common media header and metadata CRC are parsed once. CODINGS presence selects the encoded parser; a malformed encoded payload is not retried as RAW. RAW checks DATA integrity before returning samples. Encoded opening retains its metadata-only behavior and carries the file offset into group preparation.

| Operation | Checks |
| --- | --- |
| `asset.encoded_len` / `encode_into` | Surface and palette agreement, single-unit geometry, lengths, placement and output capacity |
| `asset.preflight` | Metadata, supported scalar syntax, group/unit/decoded bounds, reader-equivalent work and canonical output span; no allocation |
| `EncodedImageView::open` | Metadata CRC, sections and typed metadata; no DATA scan |
| `image.groups_into` | Group geometry, static coverage, indexes and declared file alignment |
| `image.validate_groups` | The same group checks without a stored group table, under a conservative work budget |
| `image.preflight(&limits)` | Static groups, admitted scalar profiles, exact unit syntax and complete DATA integrity without decoding samples |
| `groups.validate_unit` | Declared checksum coverage, reported as bytes read |
| `unit.decode_plan` | Supported profile syntax, exact decoded length and target memory requirements |
| `plan.decode_into` | Actual target address/capacity, then reconstruction into caller storage |

Unknown nonzero coding IDs, revisions and parameters remain representable. Even known-profile bytes are not decoded during authoring; malformed streams can be preserved but fail explicit decode preflight. Checksums prove byte integrity, not valid coding syntax.

Errors from `encode_into` preserve the entire output; success preserves its unused suffix. Canonical comparison includes directory entries, padding and both checksums. `image::ImageEncodeError` is shared by RAW and encoded surface authoring. The packed `ImageAsset` facade retains its payload-validation error wrapper.

## Validation workspace

Use `groups_into` when subsequent access benefits from prepared groups. Its caller-owned table prevents repeated index parsing; the supplied `CoverageBudget` bounds geometric group/pair/selected-cell work. Capacity errors preserve workspace, while other failures may change its used prefix.

Use `validate_groups` when only a validation result is needed. It stores no group table, validates all immutable records once, then resolves them again as the coverage algorithm needs them. Every resolution charges one record visit plus its declared DATA span and the entire UNIT_INDEX section length before index/group parsing. DATA span conservatively bounds nonempty unit visits; index bytes bound selection/range scans. Geometric work is charged separately through the same budget.

Native authoring records and borrowed wire records use one group-resolution and coverage implementation. Authoring does not serialize a temporary coding/group table for validation; native records must satisfy the same field, range, reference and alignment rules as wire records.

This conservative accounting can reject a many-group image earlier than cached validation, especially when groups share a large index section. Increase the explicit budget or provide workspace instead of assuming an index guarantees cheap validation. Exhaustion never returns success. These units describe bounded work, not actual bytes read, memory usage or elapsed time. Neither validation path decodes samples or verifies DATA checksums.

## Complete preflight

`EncodedImageView::preflight(&PayloadLimits::EMBEDDED)` checks all active groups, scalar coding parameters, exact unit syntax and complete DATA integrity without allocating a group table or output samples. Group overlap, temporal references in static images, unsupported coding, malformed streams and checksum failures remain errors. Empty surfaces contain no unit stream, but their active coding ID, revision, parameters and sample layout must still be understood.

| Limit | Embedded | Host | Meaning |
| --- | --- | --- | --- |
| `max_image_groups` | 1,024 | 65,535 | Includes the implicit whole-surface group |
| `max_image_units` | 65,535 | 16,777,216 | Total stored units across groups |
| `max_decoded_bytes` | 128 KiB | 64 MiB | Tight decoded bytes of one independent unit |
| `max_image_work` | 16,777,216 | 1,073,741,824 | Conservative bounded work after metadata opening |

Use the corresponding `with_max_*` builders to set stricter or larger limits. Zero disables the corresponding resource. A tiled image may exceed the decoded-byte limit in total while each independently decoded unit fits it. Preflight does not allocate the complete image or promise that an application can retain all decoded units simultaneously.

Work includes group resolution and coverage, each unit's coded and decoded bytes during syntax checks, and one complete DATA checksum scan. Checks precede the charged work. The common envelope and metadata CRC have already been checked by `open`; they are not retroactively limited by this later budget. Actual device stride, base alignment and output capacity still belong to the requested decode plan.

`ImageSource` and `Document::push_image` accept decoded surfaces; encoded storage uses `push_encoded_image` and `replace_encoded_image`. These APIs do not implicitly select a codec, recompress samples or integrate runtime rendering.

## Grouped authoring

`EncodedImageAsset::from_groups(surface, &codings, &groups, data)` borrows all inputs. Each `UnitGroupRecord` names one coding ordinal and a DATA range, with optional tile geometry, plane selection, sparse selection and range encoding. `with_index(bytes)` supplies the combined UNIT_INDEX body; index offsets address that body, while DATA ranges address DATA. No per-unit descriptor array is generated.

```rust
use mirx::{PayloadLimits, coding::Rle, image::{
    ColorDescription, EncodedImageAsset, GroupPlanes, SampleLayout,
    SurfaceDescriptor, UnitGroupRecord,
}};

let surface = SurfaceDescriptor::new(
    3, 3, SampleLayout::NV12, ColorDescription::BT709_YUV_LIMITED,
).unwrap();
let codings = [Rle::new().record()];
let groups = [
    UnitGroupRecord::new(0, 0..2).unwrap().with_planes(GroupPlanes::Plane(0)),
    UnitGroupRecord::new(0, 2..4).unwrap().with_planes(GroupPlanes::Plane(1)),
];
// Nine luma bytes followed by eight interleaved chroma bytes after decoding.
let data = [0x88, 16, 0x87, 128];
let asset = EncodedImageAsset::from_groups(surface, &codings, &groups, &data);
asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
let mut payload = [0; 256];
let len = asset.encode_into(&mut payload).unwrap();
assert_eq!(asset.matches_payload(&payload[..len]), Ok(true));
```

Explicit group arrays remain explicit, including a one-group array. Empty arrays are invalid. Empty index bytes omit UNIT_INDEX; fixed-size units need no range index. Variable-size units use `UnitIndexEncoding::encode_into` with the same alignment as their group. Sparse selection bytes precede range bytes within each group's index range. The writer preserves caller-selected index encodings and DATA padding, and aligns the DATA origin to the maximum group requirement. An asset-level alignment override is only available for the single-stream constructor; combining it with explicit groups is an error. `asset.input_alignment()` returns the checked effective alignment.

Low-level encoding checks records, profile references, static reference rules and exact DATA/index consumption before writing. Full static coverage and codec support remain bounded `preflight` checks: overlapping or incomplete groups cannot enter typed Document edits. Group count and minimum native table size are checked before table scans; output-span work is charged before index resolution. Single-stream default omission and bytes remain unchanged. Integrity covers all DATA; this writer does not emit indexed integrity records.

## Container reading

`ChunkRef::image` returns `Result<Option<ImageRef>, ImageReadError>`. Other chunk types return `None` without interpreting payload bytes. IMAGE retains its actual chunk offset; RAW alignment checks occur while opening samples, and encoded alignment checks occur during group preparation or preflight.

`Reader::open` uses embedded limits to preflight every critical IMAGE. `Reader::open_with` accepts custom `ReadOptions` with `PayloadLimits`; `validate_known_payloads` explicitly checks noncritical payloads as well. Encoded validation includes profile syntax and DATA integrity, not just metadata. Failures retain chunk location plus the group/unit location when applicable.

Noncritical unknown coding can be inspected as encoded metadata and preserved as bytes. It does not pass explicit semantic validation. No reader path implicitly allocates a decoded image; resolve the borrowed groups and provide an appropriately aligned output buffer to the selected unit's decode plan.

## Document access and relocation

`Document::image(id)` returns the same `ImageRef` distinction, including borrowed RAW surfaces from promoted FLAT nodes. Encoded access keeps its metadata-only contract. Inserting complete encoded payload bytes with `push_raw` and `RawChunkPolicy::infer()` checks syntax, integrity and document limits before changing state. Unknown profiles require explicit raw relocation and critical-understanding assumptions; metadata inspection grants neither.

Encoded primary hints contain the surface's sample layout and logical dimensions with stride zero. RAW hints retain the first stored plane's stride. A decode target chooses its own pitch and allocation extent; compressed DATA has no pixel-row stride.

`EncodedImageView::input_alignment()` scans group declarations and returns their maximum power-of-two alignment, defaulting to one. It does not check coverage, codec syntax or actual addresses. Document's writer uses this value to align the absolute DATA origin after chunk insertion or reordering. Payload bytes remain unchanged, and file-offset alignment does not imply that a `Vec<u8>` or embedded byte slice has an aligned base pointer. Encoded storage cannot be demoted to FLAT without an explicit external decode and replacement.

## Typed encoded edits

`push_encoded_image(&asset)` and `push_encoded_image_with_flags(&asset, flags)` store borrowed encoded input as one final owned payload after validation. `get_mut(id)?.replace_encoded_image(&asset)` preserves chunk identity and flags and updates primary hints. Structural edit gates run before candidate validation; malformed syntax, unsupported profiles or exceeded limits leave the document unchanged.

`asset.preflight(&limits)` shares scalar profile and unit checks with the encoded reader. Its work charge includes the future reader's group-resolution, coverage, syntax and DATA checksum budget, plus the complete canonical output span. Large alignment padding therefore cannot authorize an unbounded allocation merely because the decoded image is small. The gate allocates neither payload nor decoded samples; ordinary low-level `encode` remains available for metadata-valid opaque coding.

An exact canonical replacement keeps source or owned storage and dirty state, with zero allocation. If identical bytes occupy a misaligned source position, replacement creates owned storage so the writer can repair placement. Other successful replacements allocate one final payload, with no decoded staging image or codec workspace.
