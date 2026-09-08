# Encoded images

`EncodedImageAsset` wraps already encoded DATA with a surface descriptor, coding records and optional groups/indexes. Metadata authoring, integrity checking and sample decoding are separate operations. `encoded_len`, `encode_into` and `matches_payload` allocate nothing; `encode` allocates one payload.

## Mirui runtime handoff

`mirui::render::texture::Texture::plan_mirx` connects an IMAGE decode plan to renderer storage without hiding group or codec scratch allocations. The caller supplies `[Option<UnitGroup>]`, an output buffer satisfying the reported address alignment, and one reusable workspace. RAW sources use the same output layout transfer; `Texture::from_mirx` retains the zero-copy path when the RAW layout already matches the default requirements.

```rust
use mirui::render::texture::{MirxTextureOptions, Texture};
use mirx::types::ByteAlignment;

#[repr(align(64))]
struct Output([u8; 4096]);

let options = MirxTextureOptions::new().with_requirements(
    mirx::image::SurfaceRequirements::new()
        .with_base_alignment(ByteAlignment::new(64).unwrap())
        .with_plane_alignment(ByteAlignment::new(64).unwrap())
        .with_stride_multiple(64),
);
let mut groups = [None; 8];
let plan = Texture::plan_mirx(bytes, options, &mut groups)?;
assert!(plan.output_len() <= 4096);
let mut output = Output([0; 4096]);
let mut workspace = [0; 1024];
let texture = plan.decode_into(&mut output.0, &mut workspace)?;
```

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

The encoder receives tight logical sample bytes, not physical row padding. PIXEL accepts RGB888/RGBA8888 samples; RLE and LZ4 operate on bytes. For a joint YUV byte stream, concatenate tight rows in plane-index order. Indexed color tables are separate metadata, supplied with `with_color_table`; they are not compressed with the index plane. No color conversion or automatic codec selection occurs.

## Frequency profiles

`Frequency::reversible()` combines reversible YCoCg-R color decorrelation where applicable with an 8×8 integer Haar lifting transform and canonical zero-run/signed-varint coefficient coding. `Frequency::quantized(quality)` uses deterministic per-band quantization for qualities 1 through 100. It is a distinct lossy profile even at quality 100; alpha and index components remain exact. The stream is MIRX coefficient syntax, not a JPEG container or JPEG entropy stream.

```rust
use mirx::coding::{Frequency, FrequencyGeometry};

let geometry = FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 4, 2).unwrap();
let samples = [
    12, 34, 56, 255, 78, 90, 12, 128,
    34, 56, 78, 64, 90, 12, 34, 0,
    56, 78, 90, 255, 12, 34, 56, 128,
    78, 90, 12, 64, 34, 56, 78, 0,
];
let codec = Frequency::quantized(75).unwrap();
let mut stream = [0; 512];
let stream_len = codec.encode_into(geometry, &samples, &mut stream).unwrap();
let mut params = [0];
let asset = EncodedImageAsset::new(
    SurfaceDescriptor::new(4, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap(),
    codec.record_into(&mut params),
    &stream[..stream_len],
);
asset.preflight(&mirx::reader::PayloadLimits::EMBEDDED).unwrap();
```

Revision 1 accepts I8, A8, L8, RGB888, RGBA8888, BGRA8888 and the 8-bit planes of I420, YV12, NV12 and NV21. Sub-byte samples, RGB565, XRGB8888, P010 and P016 are rejected instead of being treated as byte lanes. A multi-plane unit concatenates one independently encoded plane stream per selected plane in derived plane order. Plane dimensions and component counts delimit those streams without stored length fields. Partial edge blocks repeat the nearest sample instead of injecting an artificial zero border. Every 8×8 block resets its coefficient state, and IMAGE groups remain the independently addressable unit boundary. Progressive-band access is not advertised by this revision.

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
| `asset.encoded_len` / `encode_into` | Surface/palette agreement, group/index structure, checksum partitions, lengths and placement; encoding also checks output capacity |
| `asset.preflight` | Metadata, supported scalar syntax, group/unit/decoded bounds, reader-equivalent work and canonical output span; no allocation |
| `EncodedImageView::open` | Metadata CRC, sections and typed metadata; no DATA scan |
| `image.groups_into` | Group geometry, static coverage, indexes and declared file alignment |
| `image.validate_groups` | The same group checks without a stored group table, under a conservative work budget |
| `image.preflight(&limits)` | Static groups, admitted scalar profiles, exact unit syntax and complete DATA integrity without decoding samples |
| `groups.validate_unit` | Declared checksum coverage, reported as bytes read |
| `unit.decode_plan` | Supported profile syntax, exact decoded length and target memory requirements |
| `plan.decode_into` | Actual target address/capacity, then reconstruction into caller storage |

Unknown nonzero coding IDs, revisions and parameters remain representable. Even known-profile bytes are not decoded during authoring; malformed streams can be preserved but fail explicit decode preflight. Checksums prove byte integrity, not valid coding syntax.

Errors from `encode_into` preserve the entire output; success preserves its unused suffix. Canonical comparison includes directory entries, padding, metadata CRC and every declared DATA checksum. `image::ImageEncodeError` is shared by RAW and encoded surface authoring. The packed `ImageAsset` facade retains its payload-validation error wrapper.

## Validation workspace

Use `groups_into` when subsequent access benefits from prepared groups. Its caller-owned table prevents repeated index parsing; the supplied `CoverageBudget` bounds geometric group/pair/selected-cell work. Capacity errors preserve workspace, while other failures may change its used prefix.

Use `validate_groups` when only a validation result is needed. It stores no group table, validates all immutable records once, then resolves them again as the coverage algorithm needs them. Every resolution charges one record visit plus its declared DATA span and the entire UNIT_INDEX section length before index/group parsing. DATA span conservatively bounds nonempty unit visits; index bytes bound selection/range scans. Geometric work is charged separately through the same budget.

Native authoring records and borrowed wire records use one group-resolution and coverage implementation. Authoring does not serialize a temporary coding/group table for validation; native records must satisfy the same field, range, reference and alignment rules as wire records.

This conservative accounting can reject a many-group image earlier than cached validation, especially when groups share a large index section. Increase the explicit budget or provide workspace instead of assuming an index guarantees cheap validation. Exhaustion never returns success. These units describe bounded work, not actual bytes read, memory usage or elapsed time. Neither validation path decodes samples or verifies DATA checksums.

## Complete preflight

`EncodedImageView::preflight(&PayloadLimits::EMBEDDED)` checks all active groups, scalar coding parameters, exact unit syntax and complete DATA integrity without allocating a group table or output samples. Group overlap, temporal references in static images, unsupported coding, malformed streams and checksum failures remain errors. `FRAME_DELTA` is recognized by the shared scalar layer but requires `ReferenceMode::Previous`, so it is rejected in static IMAGE and admitted only by bounded FRAMES replay. Empty surfaces contain no unit stream, but their active coding ID, revision, parameters and sample layout must still be understood.

| Limit | Embedded | Host | Meaning |
| --- | --- | --- | --- |
| `max_raster_groups` | 1,024 | 65,535 | Includes the implicit whole-surface group |
| `max_raster_units` | 65,535 | 16,777,216 | Total stored units across groups |
| `max_decoded_bytes` | 128 KiB | 64 MiB | Tight decoded bytes of one independent unit |
| `max_raster_work` | 16,777,216 | 1,073,741,824 | Conservative bounded work after metadata opening |

Use the corresponding `with_max_*` builders to set stricter or larger limits. Zero disables the corresponding resource. A tiled image may exceed the decoded-byte limit in total while each independently decoded unit fits it. Preflight does not allocate the complete image or promise that an application can retain all decoded units simultaneously.

Raster limits describe sample processing independently of IMAGE, glyph or frame semantics. Shared admission accumulates group, unit and work costs across surfaces; DATA integrity is charged separately by the complete media caller. Adding another surface does not reset those counters or require repeating a shared DATA checksum scan.

Work includes group resolution and coverage, each unit's coded and decoded bytes during syntax checks, frequency coefficient transforms where present, and one complete DATA checksum scan. Checks precede the charged work. The common envelope and metadata CRC have already been checked by `open`; they are not retroactively limited by this later budget. Actual device stride, base alignment and output capacity still belong to the requested decode plan.

`ImageSource` and `Document::push_image` accept decoded surfaces; encoded storage uses `push_encoded_image` and `replace_encoded_image`. These APIs do not implicitly select a codec, recompress samples or integrate runtime rendering.

## Grouped authoring

`EncodedImageAsset::from_groups(surface, &codings, &groups, data)` borrows all inputs. Each `UnitGroupRecord` names one coding ordinal and a DATA range, with optional tile geometry, plane selection, sparse selection and range encoding. `with_unit_index(bytes)` supplies the combined UNIT_INDEX body; index offsets address that body, while DATA ranges address DATA. No per-unit descriptor array is generated.

```rust
use mirx::{reader::PayloadLimits, coding::Rle, image::{
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

Low-level encoding checks records, profile references, static reference rules and exact DATA/index consumption before writing. Full static coverage and codec support remain bounded `preflight` checks: overlapping or incomplete groups cannot enter typed Document edits. Group count and minimum native table size are checked before table scans; output-span work is charged before index resolution. Single-stream default omission and bytes remain unchanged.

## Checksum partitions

The default `DataIntegrity::Whole` uses one four-byte DATA CRC trailer. `asset.with_integrity(DataIntegrity::Indexed(&ends))` writes a required INTEGRITY section instead, with one twelve-byte range/CRC record per partition and no whole-DATA trailer. `DataIntegrity` is in `mirx::media`. Ends are cumulative offsets relative to DATA, starting implicitly at zero; they must strictly increase and finish at DATA length. Empty DATA accepts an empty partition list. The writer computes payload-relative offsets and CRCs after placement, without allocating a temporary table.

For DATA containing two four-byte units, `Indexed(&[4, 8])` allows each unit's checksum to read four bytes. If the second unit starts at byte 64, `Indexed(&[64, 68])` folds the gap into the first partition: checking the first unit reads 64 bytes, while checking the second reads four. `Indexed(&[4, 64, 68])` isolates the gap but costs another twelve-byte record. No per-unit checksum is inserted automatically.

`groups.validate_unit(group, ordinal)` verifies only intersecting partitions and reports their total byte count. Unrelated DATA corruption does not invalidate a disjoint local request; complete `image.preflight`, critical Reader opening and typed Document admission still verify all DATA. This is borrowed-slice validation, not a flash transport or a promise that opening critical assets performs only local reads.

Partitions are covered by the metadata CRC. Author preflight bounds their native table footprint before scanning them, then charges canonical output size and one complete DATA checksum pass. Canonical comparison, caller-buffer encoding, typed replacement and no-op detection retain their allocation guarantees in both modes.

## RAW units in grouped storage

`CodingRecord::RAW` stores tight logical samples with revision 1 and no parameters. Explicit groups can mix RAW units with PIXEL, RLE, LZ4 or frequency-coded units, or retain independently addressed raw tiles. A RAW unit contains selected planes in original plane-index order; each plane contains tight local rows. Its exact byte count comes from the same geometry used by compressed units. Unknown revisions, nonempty parameters and short or long sample streams are rejected before output writes.

`decode_plan` transfers RAW bytes through the shared strided output path, with no tight staging buffer or heap. Output address, plane alignment, allocation extent and stride are independent of stored input layout. Row tails and allocation padding are normalized exactly as for compressed units. The caller chooses RAW where compression is not useful; this API does not make that size comparison automatically.

Whole-surface RAW without independent groups uses `RawImageAsset`, omitting CODINGS. The single-stream encoded constructor rejects RAW instead of writing redundant metadata. An IMAGE with explicit RAW groups is still an `ImageRef::Encoded`: group/index addressing and a decode plan remain necessary, and opening does not claim a directly borrowed contiguous surface.

## Complete scalar reconstruction

`MediaPayload::data_check_plan(range)` separates checksum budgeting from execution. The payload-relative request must lie inside one DATA section. `byte_len()` reports the exact DATA bytes that `verify()` will scan: intersecting indexed partitions or all DATA bodies with whole-DATA integrity. Empty ranges cost zero. Planning reads metadata only, so corrupted DATA may still produce a plan; verification is required before consumption. This borrowed-slice operation does not implement partial file I/O.

Shared encoded-section binding keeps directory selection separate from coding/group body validation. Whole preflight and reconstruction charge every DATA body covered by complete media validation, including bodies outside the selected surface. That total is derived once from directory metadata and adds no wire field. ROI plans retain their explicit partition or whole-DATA coverage; an empty ROI performs no sample checksum work.

Prepared `ImageGroups` expose `decode_plan(requirements, limits)` for the complete surface. Planning reuses validated group slots, checks every scalar unit and complete DATA integrity, and returns an `ImageDecodePlan` without storing an expanded unit-plan table or allocating decoded bytes. RAW, PIXEL, RLE, LZ4 and both frequency profiles share this path.

`SurfaceView::access_capabilities()` always reports direct borrowing and reports whole-surface, logical-row and exact-region access only when every plane uses known linear storage flags. `ImageGroups::access_capabilities()` reports whole-surface and exact-region scalar reconstruction only after checking every group's coding ID, revision, parameters and sample layout; unknown profiles return the same typed group error as decode preflight. `UnitGroup::access_capabilities()` additionally reports independently addressable units when its grid contains more than one cell. None of these sources advertises progressive or direct-upload access. Carrying a compute or upload execution request does not create an implementation.

`memory_plan()` describes the final output, including required address alignment and row stride. `workspace_requirements()` describes the largest tight decoded unit, reused across units at scalar alignment one. A tiled stream can bound this staging buffer by tile size; a whole-image stream requires whole-image staging. This path does not promise zero staging or direct GPU execution.

`decode_into(output, workspace)` validates both caller buffers before changing either, initializes the final allocation to zero and reconstructs every unit through shared exact placement. Output padding is zero and both buffer suffixes are preserved. Returned sample planes borrow output; the indexed color table remains borrowed from the encoded source. No color conversion occurs.

Group/unit and tight per-unit decoded limits remain explicit. The work budget charges output initialization, one complete DATA checksum scan, unit syntax preflight, execution-time re-preflight/replay, placement and bounded unit visits. `work()` reports the charge; it is not a cycle count. Earlier metadata/group preparation and coverage are not repeated or included in this request charge. Unsupported coding, corrupt input, exhausted limits and caller-buffer errors are rejected before final output changes.

## Selected-region reconstruction

`groups.decode_region_plan(region, requirements, limits)` returns the same `ImageDecodePlan` for an exact cropped surface. A full-width one-row region requests a row when its chroma boundaries are valid. The plan projects requests into planar group coordinates, selects intersecting units and checks their scalar syntax. Complete intersecting units are decoded; only their requested samples are copied. Unknown profiles or malformed streams outside the selection are not decoded by this path. Earlier group preparation still validates complete static coverage and all index metadata.

`region_plan()` retains source and cropped geometry. `input_byte_len()` counts selected encoded bytes without alignment gaps; `checksum_byte_len()` counts actual DATA verification, including intersecting partition expansion and required gaps. Shared partitions are verified once and whole-DATA integrity scans complete DATA at most once. Empty regions select no units and require no checksum or workspace bytes. These counts describe borrowed input processing, not partial disk or Flash I/O.

The workspace is the largest complete selected unit, even for a tiny crop. Final allocation, exact YUV boundaries, sub-byte placement, palette borrowing and binding-error atomicity use the same rules as whole-image reconstruction. The work charge includes spatial-query bounds, selected syntax checks, coalesced checksum scans, replay and cropped output initialization. Unit limits apply to the selection; the group limit still bounds the prepared image's groups. Streamed reads, backend negotiation, acceleration and tool/runtime integration remain separate operations.

## Placing decoded units

`UnitGroup::units_in(region)` queries complete units intersecting a rectangle in the group's grid coordinates: surface pixels for joint groups, plane elements for planar groups. Use `Region::for_plane` or `RegionMemoryPlan::plane_region` when projecting a surface request into a planar group. Out-of-bounds regions fail; the returned units are not clipped.

`RegionUnits` retains borrowed group/index state and uses rank jumps to skip unrelated columns and absent rows. Construction performs bounded rank lookups; `work_bound()` bounds subsequent traversal probes. A probe retains the selection/index's bounded binary-search or checkpoint cost. Size hints are conservative, and skipping yielded units is sequential. Syntax, integrity, decoded bytes and I/O costs are separate from this metadata query.

`SurfaceDescriptor::region_plan(region, requirements)` returns a `RegionMemoryPlan` retaining original source coordinates and a cropped `SurfaceMemoryPlan`. The cropped descriptor preserves sample layout, color, flags and pixel aspect. Interior subsampled-plane boundaries must be exact; odd outer edges are allowed. Packed 1/2/4-bit sample origins remain precise without byte rounding.

`DecodedUnit::copy_region_into(output, region_plan)` writes only the intersection of an already decoded unit with the requested crop. Other samples, planes, padding and suffix bytes stay unchanged. `SurfaceView::copy_region_into` transfers a complete RAW crop and clears output padding, retaining its borrowed palette. These paths share plane-coordinate and sample-bit placement; they do not select encoded units, reduce a codec's staging needs or imply partial Flash reads.

`DecodedUnit::copy_into(output, whole_surface_plan)` writes selected samples into a checked whole-surface allocation at their original positions. Planar units retain the original plane index and chroma coordinates; they are not renumbered as plane zero. The target descriptor must match the source surface.

Only the unit's sample bits change. Other samples, absent planes, row padding, allocation rows, inter-plane gaps and the output suffix remain untouched. For 1/2/4-bit layouts, masked edge writes preserve neighbouring pixels in the same byte. Byte-aligned interiors use bulk copies; unaligned rows transfer shifted bytes without per-pixel staging.

The caller can reuse one decoded-unit buffer across RAW, PIXEL, RLE, LZ4 and frequency-coded units. This requires both that buffer and the final output allocation; a whole-image unit still needs whole-image temporary output with this approach. Placement does not allocate, select units or provide failure-atomic multi-unit execution. Integrity, required-unit preflight, complete coverage and final padding initialization belong to the enclosing request.

## Container reading

`ChunkRef::image` returns `Result<Option<ImageRef>, ImageReadError>`. Other chunk types return `None` without interpreting payload bytes. IMAGE retains its actual chunk offset; RAW alignment checks occur while opening samples, and encoded alignment checks occur during group preparation or preflight.

`Reader::open` uses embedded limits to preflight every critical IMAGE. `Reader::open_with` accepts custom `ReadOptions` with `PayloadLimits`; `validate_known_payloads` explicitly checks noncritical payloads as well. Encoded validation includes profile syntax and DATA integrity, not just metadata. Failures retain chunk location plus the group/unit location when applicable.

Noncritical unknown coding can be inspected as encoded metadata and preserved as bytes. It does not pass explicit semantic validation. No reader path implicitly allocates a decoded image; resolve the borrowed groups and provide an appropriately aligned output buffer to the selected unit's decode plan.

## Document access and relocation

`Document::get(id)?.image()` returns the same `ImageRef` distinction, including borrowed RAW surfaces from promoted FLAT nodes. Encoded access keeps its metadata-only contract. Inserting complete encoded payload bytes with `push_extension` checks syntax, integrity and document limits before changing state. Unknown profiles require explicit extension relocation and critical-understanding assumptions; metadata inspection grants neither.

Encoded primary hints contain the surface's sample layout and logical dimensions with stride zero. RAW hints retain the first stored plane's stride. A decode target chooses its own pitch and allocation extent; compressed DATA has no pixel-row stride.

`EncodedImageView::input_alignment()` scans group declarations and returns their maximum power-of-two alignment, defaulting to one. It does not check coverage, codec syntax or actual addresses. Document's writer uses this value to align the absolute DATA origin after chunk insertion or reordering. Payload bytes remain unchanged, and file-offset alignment does not imply that a `Vec<u8>` or embedded byte slice has an aligned base pointer. Encoded storage cannot be demoted to FLAT without an explicit external decode and replacement.

## Typed encoded edits

`push_encoded_image(&asset)` and `push_encoded_image_with_flags(&asset, flags)` store borrowed encoded input as one final owned payload after validation. `get_mut(id)?.replace_encoded_image(&asset)` preserves chunk identity and flags and updates primary hints. Structural edit gates run before candidate validation; malformed syntax, unsupported profiles or exceeded limits leave the document unchanged.

`asset.preflight(&limits)` shares scalar profile and unit checks with the encoded reader. Its work charge includes the future reader's group-resolution, coverage, syntax and DATA checksum budget, plus the complete canonical output span. Large alignment padding therefore cannot authorize an unbounded allocation merely because the decoded image is small. The gate allocates neither payload nor decoded samples; ordinary low-level `encode` remains available for metadata-valid opaque coding.

An exact canonical replacement keeps source or owned storage and dirty state, with zero allocation. If identical bytes occupy a misaligned source position, replacement creates owned storage so the writer can repair placement. Other successful replacements allocate one final payload, with no decoded staging image or codec workspace.
