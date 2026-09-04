# mirx

MIRX is mirui's ahead-of-time binary asset format. It packages images, fonts, vector scenes, metadata, palettes, and frame sequences into deterministic bytes that can be embedded with `include_bytes!` and consumed directly on constrained targets.

The crate is `no_std + alloc`, has no external dependencies, and separates the allocation-free runtime path from source-backed authoring.

![MIRX architecture: assets are encoded into FLAT or CHUNK containers, then consumed through Reader or edited through Document](docs/architecture.svg)

## Reading path

![Four-step MIRX documentation path from the file model to checked output](docs/reading-path.svg)

**Jump to:** [container model](#container-model) · [concrete byte map](#one-file-five-resources) · [runtime reading](#runtime-reading) · [document authoring](#authoring-a-document) · [copy-on-write editing](#copy-on-write-editing) · [checked encoding](#checked-encoding) · [command-line workflow](#command-line-workflow)

## Design at a glance

- **One format, two layouts.** FLAT is the compact single-image path; CHUNK is an ordered container for heterogeneous or repeated assets.
- **Borrow first.** `Reader` validates structure and returns views into the original byte slice.
- **Materialize only what changes.** `Document` keeps untouched payloads borrowed and owns only inserted or edited data.
- **Make unsafe assumptions explicit.** Opaque relocation, critical semantics, reserved bits, and future file semantics require named policies.
- **Plan before writing.** Size, offsets, alignment, representability, and CRCs are checked before output bytes are emitted.

## Two paths through the same bytes

![Reader is the zero-allocation runtime path; Document is the copy-on-write authoring path](docs/read-or-edit.svg)

| Goal | Entry point | Allocation model | Result |
| --- | --- | --- | --- |
| Inspect or render | `Reader::open` | Zero allocation | Borrowed container and payload views |
| Validate untrusted input | `Reader::open_with` + `validate_known_payloads` | Zero allocation | Bounded structural and typed validation |
| Modify an existing file | `Document::open` / `from_vec` | CHUNK: one node table | Source-backed, copy-on-write document |
| Build a new file | `Document::new` / `new_flat` | Owned authoring state | Checked FLAT or CHUNK output |

`Reader` and `Document` enforce the same wire rules. The distinction is intent: runtime code reads; tools and build pipelines author.

## Container model

![Comparison of MIRX FLAT and CHUNK byte layouts](docs/container-layouts.svg)

### FLAT

FLAT stores exactly one image. Its header carries format, dimensions, and stride; the plane lengths are derived from that geometry. The main image plane is stored after the header, with an inline palette or alpha plane when required by the format. It is the smallest representation for a standalone image.

### CHUNK

Canonical CHUNK output stores an ordered descriptor table followed by aligned payload ranges. Each descriptor carries a type, flags, offset, and size. The reader also accepts valid relocated tables and payload ranges. A header-level primary selection can expose display hints without decoding every payload.

The six standard payload types are `IMAGE`, `FONT`, `VECTOR`, `META`, `PALETTE`, and `FRAMES`. `ChunkType` also represents every nonzero custom `u16`, allowing unknown payloads to be inspected and preserved.

Primary display hints use a 16-bit `image::SampleLayout`, including planar YUV identifiers. `PrimaryHints::new(layout, width, height, stride)` stores logical dimensions and a byte stride; `SampleLayout::NONE` denotes a primary without a fixed pixel layout. The CHUNK header remains 44 bytes, with the sample-layout field at bytes 22–23.

### One file, five resources

![Exact byte allocation for a MIRX CHUNK file containing two images, two fonts, and one vector scene](docs/binary-allocation.svg)

This 1,725-byte layout contains a 44-byte CHUNK header, five 16-byte descriptors, 1,597 payload bytes, and two 2-byte alignment gaps. The two sectioned IMAGE payloads occupy 1,060 and 114 bytes. The VECTOR scene references the preceding IMAGE and FONT chunks by table index without embedding their bytes again.

## Payload families

![The six standard MIRX payload families and their access models](docs/payload-families.svg)

| Type | Purpose | Read access | Authoring value |
| --- | --- | --- | --- |
| `IMAGE` | Packed, indexed, alpha, and planar YUV surfaces | `SurfaceView` | `ImageAsset` / `RawImageAsset` |
| `FONT` | Glyph metrics and atlas data | Bounded decode | `Font` |
| `VECTOR` | Ordered scene operations | Bounded decode | `Scene` |
| `META` | Ordered text, bytes, and extension values | `MetaView` | `Meta` |
| `PALETTE` | Ordered RGBA colors | `PaletteView` | `Palette` |
| `FRAMES` | Atlas regions or timed animation frames | `FramesView` | `FramesAsset` |

IMAGE, META, PALETTE, and FRAMES expose borrowed views. FONT and VECTOR use `decode_font` and `decode_vector` to make their owned allocation visible at the call site.

## Runtime reading

`Reader` validates the common header, exact logical length, chunk table, payload ranges, primary selection, and configured resource limits. Chunk iteration and borrowed typed views continue to reference the input bytes.

```rust
use mirx::{ChunkType, PayloadLimits, Reader};

fn inspect(bytes: &[u8]) {
    let reader = Reader::open(bytes).expect("valid MIRX container");

    for chunk in reader.chunks() {
        let _descriptor = (
            chunk.index(),
            chunk.chunk_type().raw(),
            chunk.payload().len(),
        );

        if chunk.chunk_type() == ChunkType::IMAGE {
            let image = chunk
                .image()
                .expect("valid IMAGE payload")
                .expect("IMAGE type");
            for plane in image.planes() {
                assert!(plane.memory().stride() >= plane.geometry().minimum_stride().unwrap());
            }
        }
    }

    reader
        .validate_known_payloads(&PayloadLimits::EMBEDDED)
        .expect("valid standard payloads");
}
```

`ReadOptions` configures chunk-count limits, payload limits, and trailing-byte handling. `PayloadLimits::EMBEDDED` is the bounded default; `PayloadLimits::HOST` is the explicit larger profile for host tools. FONT and VECTOR provide zero-allocation preflight before bounded decoding.

`parse`, `parse_flat`, and `parse_chunk` expose owned container metadata and packed image views. `Reader` exposes sectioned IMAGE surfaces, including planar YUV, without allocating container metadata.

## Image geometry

IMAGE uses a 32-byte media header, 16-byte section entries, a 32-byte SURFACE record, optional PLANES and COLOR_TABLE sections, DATA, and a trailing CRC. Tight RAW planes derive their stride and offsets from the surface; padded allocation extents, strides, offsets, and alignment use explicit plane records. FLAT keeps its compact packed-image layout.

`Document::push_image` and `DocumentChunkMut::replace_image` accept `ImageSource`: packed `ImageAsset`, planar `RawImageAsset`, `RawImageView`, or `SurfaceView`. Typed reads return the same borrowed `SurfaceView` for all IMAGE layouts. Its `packed()` projection returns `None` when the color or storage contract cannot be expressed as a packed image.

`RawImageView::open_at` validates file-relative DATA and plane alignment. `SurfaceView::data_addresses_are_aligned` checks the actual in-memory plane addresses; valid file offsets alone do not make a byte slice suitable for GPU access. `SurfaceRequirements` plans padded allocation dimensions, row strides, and plane addresses without changing the logical image dimensions.

`SurfaceView::copy_into(output, plan)` transfers logical RAW samples into a caller-owned allocation. Source padding is ignored; destination padding and unused sub-byte row bits become zero. All validation precedes writes, and any output suffix remains unchanged. The returned view borrows the output planes and retains the source's indexed color table without copying it. No allocation or color conversion occurs.

```rust
use mirx::image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor, SurfaceRequirements};

#[repr(align(64))]
struct Buffer([u8; 256]);

let surface = SurfaceDescriptor::new(
    2, 2, SampleLayout::NV12, ColorDescription::BT709_YUV_LIMITED,
).unwrap();
let image = RawImageAsset::new(surface, &[&[16; 4], &[128; 2]]).view().unwrap();
let plan = surface.memory_plan(
    SurfaceRequirements::new()
        .with_base_alignment(64)
        .with_plane_alignment(64)
        .with_stride_multiple(64),
).unwrap();
let mut buffer = Buffer([0; 256]);
let copied = image.copy_into(&mut buffer.0, plan).unwrap();
assert_eq!(copied.surface().width(), 2);
assert_eq!(copied.plane(0).unwrap().memory().stride(), 64);
assert!(copied.data_addresses_are_aligned());
```

`ColorFormat::bits_per_pixel()` is the canonical main-plane pixel depth. `ColorFormat::minimum_stride(width)` derives the smallest valid row stride, including sub-byte indexed and alpha formats.

```rust
use mirx::ColorFormat;

assert_eq!(ColorFormat::A1.bits_per_pixel(), 1);
assert_eq!(ColorFormat::A1.minimum_stride(13), Some(2));
assert_eq!(ColorFormat::RGB565.bits_per_pixel(), 16);
assert_eq!(ColorFormat::RGB565.minimum_stride(13), Some(26));
assert_eq!(ColorFormat::RGBA8888.bits_per_pixel(), 32);
assert_eq!(ColorFormat::RGBA8888.minimum_stride(13), Some(52));
```

Formats with a separate palette or alpha plane report the depth of the main plane. `ColorFormat::extra_size(width, height, stride)` calculates the required extra-plane byte count.

## Font codepoint tables

`FontCodepoints` borrows a CODEPOINTS section body: strictly increasing little-endian `u32` Unicode scalar values. It rejects surrogates, out-of-range values, duplicates, unordered records, and partial entries. Glyph ordinals come from table positions, so raster representations can share one Unicode directory. `get`, `binary_search`, and double-ended iteration require no allocation or pointer alignment.

```rust
use mirx::FontCodepoints;

let bytes = [0x41, 0, 0, 0, 0x2d, 0x4e, 0, 0];
let codepoints = FontCodepoints::open(&bytes).unwrap();
assert_eq!(codepoints.binary_search('A'), Ok(0));
assert_eq!(codepoints.get(1), Some('中'));
assert_eq!(codepoints.binary_search('B'), Err(1));
```

## Authoring a document

![Document owns collection operations while DocumentChunkMut scopes edits to one existing chunk](docs/document-operations.svg)

`Document` owns operations that change the collection or container-wide state:

- query with `chunks`, `get`, and `chunks_of_type`;
- append or position raw chunks with `push_raw`, `insert_raw_before`, and `insert_raw_after`;
- append standard payloads with `push_image`, `push_font`, `push_vector`, `push_meta`, `push_palette`, and `push_frames`;
- remove or reorder with `remove`, `move_before`, and `move_after`;
- manage the primary chunk with `set_primary`, `set_primary_with_hints`, and `clear_primary`;
- convert layouts with `promote_to_chunk` and `demote_to_flat`.

`Document::get_mut(id)` returns a `DocumentChunkMut` scoped to one existing chunk. The handle exposes `set_type`, `set_flags`, `set_raw_policy`, `replace_raw`, and all typed replacement or transactional edit methods. This keeps document-wide operations separate from chunk-local mutation.

```rust
extern crate alloc;

use alloc::borrow::Cow;

use mirx::{ColorFormat, Document, EncodeOptions, ImageAsset};

let pixels = [0_u8, 64, 128, 255];
let stride = ColorFormat::A8.minimum_stride(2).unwrap();
let image = ImageAsset::new(
    2,
    2,
    ColorFormat::A8,
    stride,
    Cow::Borrowed(&pixels),
);

let mut document = Document::new();
let image_id = document.push_image(&image).unwrap();
document.set_primary(image_id).unwrap();

let options = EncodeOptions::new();
let encoded_len = document.encoded_len(&options).unwrap();
let mut output = alloc::vec![0_u8; encoded_len];
let written = document.encode_into(&mut output, &options).unwrap();
assert_eq!(written, encoded_len);
```

`Document::new()` and `Document::default()` create an empty CHUNK document using embedded payload limits. `Document::new_with_limits()` selects a different resource profile. `Document::new_flat(image)` starts with the compact single-image layout.

Chunk IDs remain stable across insert, remove, and reorder operations within a document session. They are not stored in the file and must not be persisted across reopen.

## Copy-on-write editing

![Untouched payloads remain borrowed while one edited payload is materialized and replaced transactionally](docs/copy-on-write.svg)

Opening a CHUNK document allocates one node table with `O(chunk_count)` entries. Payload bytes stay in the source buffer until an operation needs ownership. Typed edits decode only the selected payload, run the callback on a working value, validate and encode its replacement, then commit the change. Any failure leaves the document unchanged.

```rust,no_run
use mirx::{ChunkType, Document, MetaEntry};

# fn edit(bytes: &[u8]) -> Result<(), mirx::EditError> {
let mut document = Document::open(bytes).expect("valid MIRX container");
let meta_id = document
    .chunks_of_type(ChunkType::META)
    .next()
    .expect("META chunk")
    .id();

document
    .get_mut(meta_id)
    .expect("stable chunk id")
    .edit_meta(|meta| {
        meta.push(MetaEntry::text("locale", "en-US")).unwrap();
    })?;
# Ok(())
# }
```

`edit_*` callbacks mutate owned values. `try_edit_*` additionally keeps caller errors separate from MIRX validation and encoding errors.

### Typed chunk operations

| Type | Access on `Document` | Add on `Document` | Edit on `DocumentChunkMut` |
| --- | --- | --- | --- |
| `IMAGE` | `image` | `push_image` | `replace_image` |
| `FONT` | `decode_font` | `push_font` | `replace_font`, `edit_font`, `try_edit_font` |
| `VECTOR` | `decode_vector` | `push_vector` | `replace_vector`, `edit_vector`, `try_edit_vector` |
| `META` | `meta` | `push_meta` | `replace_meta`, `edit_meta`, `try_edit_meta` |
| `PALETTE` | `palette` | `push_palette` | `replace_palette`, `edit_palette`, `try_edit_palette` |
| `FRAMES` | `frames` | `push_frames` | `replace_frames`, `edit_frames`, `try_edit_frames` |

Typed `push_*` methods use `ChunkFlags::NONE`. Their `push_*_with_flags(value, flags)` counterparts retain explicit descriptor control.

`Meta` and `Palette` use `push`, `insert`, `replace`, and `remove`. `FramesAsset` exposes the corresponding `push_frame`, `insert_frame`, `replace_frame`, `remove_frame`, and `move_frame` methods; `Scene::push` appends an operation.

## Composing typed values

`ImageAsset::new` accepts the required geometry and main plane. `with_extra` adds an inline palette or alpha plane only when the selected format needs one.

FRAMES construction reuses the same image model:

```rust,no_run
extern crate alloc;

use alloc::{borrow::Cow, vec};
use mirx::{AnimationFrames, AnimationSettings, ColorFormat, Frame, ImageAsset};

let frame = Frame {
    source_x: 0,
    source_y: 0,
    width: 32,
    height: 32,
    target_x: 0,
    target_y: 0,
    duration_ticks: 100,
};
let atlas = ImageAsset::new(
    64,
    64,
    ColorFormat::RGBA8888,
    64 * 4,
    Cow::Borrowed(&[]),
);
let animation = AnimationFrames::new(
    atlas,
    vec![frame],
    AnimationSettings::new(64, 64, 1_000, 100),
);
# let _ = animation;
```

`AtlasFrames::new(image, frames)` represents addressable atlas regions. `AnimationFrames::new(image, frames, settings)` adds canvas, timing, and playback semantics through `AnimationSettings`.

## Checked encoding

![MIRX encoding validates policy and layout before one deterministic emit pass](docs/encode-pipeline.svg)

- `finish()` returns the exact original bytes when an opened document is unchanged. Borrowed input stays borrowed; owned input retains its allocation.
- `encoded_len()` validates the document and computes the exact output size.
- `encode_into()` writes into caller-provided storage without allocating the output and leaves the unused suffix untouched.
- `encode()` performs one exact-size output allocation after planning.
- `LayoutPolicy` selects preserve-or-promote, smallest representable, forced FLAT, or forced CHUNK output.

Modified CHUNK output is deterministic: descriptor order is stable, padding is canonical, payload alignment is checked, primary hints are derived from typed payloads when possible, and CRCs cover the defined envelope.

## Raw and future payloads

Raw mutation is explicit about assumptions that cannot be proven from opaque bytes:

```rust,no_run
use mirx::{
    ChunkFlags, ChunkType, CriticalAssumption, RawChunkInput, RawChunkPolicy,
    RelocationAssumption, ReservedBitsPolicy,
};

# let payload: &[u8] = &[];
let policy = RawChunkPolicy::infer()
    .with_relocation(RelocationAssumption::AssumeRelocatable)
    .with_critical_semantics(CriticalAssumption::AssumeCriticalUnderstood)
    .with_reserved_bits(ReservedBitsPolicy::Preserve);

let input = RawChunkInput::new(ChunkType::new(0x8000).unwrap(), payload)
    .with_flags(ChunkFlags::CRITICAL)
    .with_policy(policy);
# let _ = input;
```

- `RelocationAssumption` controls whether opaque payload bytes may move.
- `CriticalAssumption` records whether a critical custom contract is understood.
- `ReservedBitsPolicy` rejects, preserves, or normalizes reserved flag bits.
- `RawTypePolicy` grants a policy to matching source chunks during open.

Higher minor versions and nonzero file flags are preserved as read-only future semantics by default. `CompatibilityPolicy::NormalizeToCurrent` is the explicit rewrite path. Preserved trailing bytes likewise remain read-only until `discard_trailing_bytes()` is called.

## Command-line workflow

The workspace `xtask` provides inspection, validation, extraction, and guarded raw editing:

```text
cargo xtask mirx inspect <file>
cargo xtask mirx validate <file> [--known-payloads]
cargo xtask mirx extract <file> --index <n> --out <payload> [guards]
cargo xtask mirx insert <file> --type <u16> --payload <path> [--flags <u16>] [raw policy options]
cargo xtask mirx replace <file> --index <n> --payload <path> [guards] [raw policy options]
cargo xtask mirx remove <file> --index <n> [guards] [raw policy options]
cargo xtask mirx move <file> --index <n> (--before <n> | --after <n>) [guards] [raw policy options]
cargo xtask mirx set-primary <file> --index <n> [--hints <sample-layout,width,height,stride>] [guards] [raw policy options]
cargo xtask mirx clear-primary <file> [guards] [raw policy options]
```

Guards prevent an index-based script from acting on stale content:

```text
--expect-type <u16> --expect-crc <u32>
```

Raw policy options make opaque rewrite authority explicit:

```text
--assume-relocatable --assume-critical-understood
--assume-relocatable-type <u16> --assume-critical-type <u16>
--preserve-reserved-flags | --normalize-reserved-flags
```

Decimal and `0x`-prefixed values are accepted. File edits are failure-safe: the complete result is encoded to a sibling temporary file, flushed, assigned the original permissions, and atomically renamed over the input only after every check succeeds.

## Guarantees

| Property | Guarantee |
| --- | --- |
| Runtime allocation | Reader open, iteration, findings, and borrowed views allocate nothing |
| Resource control | Chunk counts, decoded records, and payload sizes are bounded by profiles |
| Edit atomicity | Failed typed or structural edits leave document state unchanged |
| No-op identity | An unchanged document returns its original bytes exactly |
| Output stability | Modified documents use deterministic ordering, padding, and metadata |
| Forward handling | Unknown types are preserved; future semantics remain read-only by default |
| Portability | `no_std + alloc`, no external dependencies, Rust 1.85 compatible |

## License

MIT.
