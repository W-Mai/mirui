# mirx

MIRX is mirui's ahead-of-time binary asset format. It stores images, fonts,
vector scenes, metadata, palettes, and frame sequences in bytes suitable for
`include_bytes!` and direct use on constrained targets.

The crate is `no_std + alloc`, has no external dependencies, and separates
allocation-free inspection from copy-on-write editing.

## Container layouts

MIRX 1.0 has two layouts:

- **FLAT** stores one image with its format, dimensions, stride, main plane, and
  optional palette or alpha plane.
- **CHUNK** stores an ordered table of typed payloads. One chunk can be selected
  as primary, with display hints in the container header.

The standard CHUNK payload types are `IMAGE`, `FONT`, `VECTOR`, `META`,
`PALETTE`, and `FRAMES`. `ChunkType` also represents any nonzero custom `u16`,
so unknown payloads can be inspected and preserved.

## Reading without allocation

`Reader` validates the container and borrows every returned byte slice. Opening,
chunk iteration, compliance findings, known-payload preflight, and borrowed
`IMAGE`, `META`, `PALETTE`, and `FRAMES` views do not allocate.

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
            assert!(image.stride() >= image.format().minimum_stride(image.width()).unwrap());
        }
    }

    reader
        .validate_known_payloads(&PayloadLimits::EMBEDDED)
        .expect("valid standard payloads");
}
```

`ReadOptions` controls the chunk-count limit, payload limits, and trailing-byte
policy. `PayloadLimits::EMBEDDED` is the default bounded profile;
`PayloadLimits::HOST` is the explicit larger profile for host tools. `FONT` and
`VECTOR` provide zero-allocation preflight and bounded owned decoding.

The original `parse`, `parse_flat`, and `parse_chunk` functions remain available
for compatibility.

## Pixel depth and row stride

`ColorFormat::bits_per_pixel()` is the canonical main-plane pixel depth.
`ColorFormat::minimum_stride(width)` derives the smallest valid byte stride from
that value, including sub-byte indexed and alpha formats.

```rust
use mirx::ColorFormat;

assert_eq!(ColorFormat::A1.bits_per_pixel(), 1);
assert_eq!(ColorFormat::A1.minimum_stride(13), Some(2));
assert_eq!(ColorFormat::RGB565.bits_per_pixel(), 16);
assert_eq!(ColorFormat::RGB565.minimum_stride(13), Some(26));
assert_eq!(ColorFormat::RGBA8888.bits_per_pixel(), 32);
assert_eq!(ColorFormat::RGBA8888.minimum_stride(13), Some(52));
```

Formats with a separate palette or alpha plane report the depth of the main
plane. `ColorFormat::extra_size(width, height, stride)` calculates the required
extra-plane byte count.

## Copy-on-write documents

`Document` opens borrowed bytes with one node-table allocation for CHUNK files.
Unchanged payloads continue to reference their original ranges. Typed mutation
materializes only the payload being changed, and every checked edit is
transactional: an error leaves document state unchanged.

```rust
extern crate alloc;

use alloc::borrow::Cow;

use mirx::{ChunkFlags, ColorFormat, Document, EncodeOptions, ImageAsset};

let pixels = [0_u8, 64, 128, 255];
let stride = ColorFormat::A8.minimum_stride(2).unwrap();
let image = ImageAsset::new(
    2,
    2,
    ColorFormat::A8,
    stride,
    Cow::Borrowed(&pixels),
    None,
);

let mut document = Document::new_chunk();
let image_id = document.push_image(ChunkFlags::NONE, &image).unwrap();
document.set_primary(image_id).unwrap();

let options = EncodeOptions::new();
let encoded_len = document.encoded_len(&options).unwrap();
let mut output = vec![0_u8; encoded_len];
let written = document.encode_into(&mut output, &options).unwrap();
assert_eq!(written, encoded_len);
```

Chunk identities returned by `Document` are stable across insert, remove, and
reorder operations during that document session. They are deliberately absent
from the wire format and must not be persisted across reopen.

Common ordered operations:

- `chunks`, `get`, `chunks_of_type`
- `push_raw`, `insert_before`, `insert_after`, `replace_raw`, `remove`
- `move_before`, `move_after`
- `set_type`, `set_flags`, `set_raw_policy`
- `set_primary`, `set_primary_with_hints`, `clear_primary`
- `promote_to_chunk`, `demote_to_flat`

Typed payload operations:

| Type | Borrowed or decoded access | Add | Replace | Transactional edit |
| --- | --- | --- | --- | --- |
| `IMAGE` | `image` → `ImageView` | `push_image` | `replace_image` | — |
| `FONT` | `font` → `Font` | `push_font` | `replace_font` | `edit_font`, `try_edit_font` |
| `VECTOR` | `vector` → `Scene` | `push_vector` | `replace_vector` | `edit_vector`, `try_edit_vector` |
| `META` | `meta` → `MetaView` | `push_meta` | `replace_meta` | `edit_meta`, `try_edit_meta` |
| `PALETTE` | `palette` → `PaletteView` | `push_palette` | `replace_palette` | `edit_palette`, `try_edit_palette` |
| `FRAMES` | `frames` → `FramesView` | `push_frames` | `replace_frames` | `edit_frames`, `try_edit_frames` |

`Meta` exposes positional insert, replace, and remove operations. `Palette` and
`FramesAsset` additionally support record reordering. `FramesAsset` keeps its
frame table and image planes borrowed until the corresponding mutable accessor
is used.

## Encoding and ownership

- `Document::finish()` returns the exact original bytes for an unchanged
  source. A borrowed source stays borrowed; an owned source retains its
  allocation.
- `encoded_len()` validates and computes the exact output size.
- `encode_into()` writes into caller storage without allocating the output
  and preserves the unused suffix.
- `encode()` performs one exact-size output allocation after planning.
- `LayoutPolicy` selects preserve-or-promote, smallest representable, forced
  FLAT, or forced CHUNK output.

Modified CHUNK output is deterministic. Payloads stay in table order, payload
alignment and padding are canonical, and primary hints are derived from typed
payloads when possible.

## Raw and future payloads

Raw mutation is explicit about assumptions that cannot be proven from opaque
bytes:

- `RelocationAssumption` controls whether a payload may move during rewrite.
- `CriticalAssumption` records whether a critical custom contract is
  understood.
- `ReservedBitsPolicy` rejects, preserves, or normalizes reserved flag bits.
- `RawTypePolicy` grants a policy to matching source chunks during open.

Higher minor versions and nonzero file flags are preserved as read-only future
semantics by default. `CompatibilityPolicy::NormalizeToCurrent` is the explicit
rewrite path. Preserved trailing bytes similarly remain read-only until
`discard_trailing_bytes()` is called.

## Command-line tools

The workspace `xtask` exposes inspection, validation, extraction, and guarded
raw editing:

```text
cargo xtask mirx inspect <file>
cargo xtask mirx validate <file> [--known-payloads]
cargo xtask mirx extract <file> --index <n> --out <payload> [guards]
cargo xtask mirx insert <file> --type <u16> --payload <path> [--flags <u16>] [raw policy options]
cargo xtask mirx replace <file> --index <n> --payload <path> [guards] [raw policy options]
cargo xtask mirx remove <file> --index <n> [guards] [raw policy options]
cargo xtask mirx move <file> --index <n> (--before <n> | --after <n>) [guards] [raw policy options]
cargo xtask mirx set-primary <file> --index <n> [--hints <format,width,height,stride>] [guards] [raw policy options]
cargo xtask mirx clear-primary <file> [guards] [raw policy options]
```

Guards:

```text
--expect-type <u16> --expect-crc <u32>
```

Raw policy options:

```text
--assume-relocatable --assume-critical-understood
--assume-relocatable-type <u16> --assume-critical-type <u16>
--preserve-reserved-flags | --normalize-reserved-flags
```

Decimal and `0x`-prefixed values are accepted. Editing commands identify chunks
by the current table index because process-local `ChunkId` values do not survive
reopen. Type and CRC guards protect scripts from acting on a stale index.

File edits are failure-safe: the complete result is encoded to a sibling
temporary file, flushed, assigned the original permissions, and atomically
renamed over the input only after every check succeeds.

## License

MIT.
