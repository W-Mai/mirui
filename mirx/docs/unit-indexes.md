# Unit indexes

`image::UnitIndex` maps a stored-unit ordinal to an exact coded byte range relative to group DATA. The group's grid and selection provide unit count and geometry. A returned range never includes bytes inserted to align the next unit. `byte_len()` is the complete physical DATA span, including inter-unit gaps but excluding any unnecessary final padding.

| Form | Index body | Lookup | Inter-unit gaps |
| --- | --- | --- | --- |
| Fixed | None | Ordinal × aligned step | Derived from fixed coded length and alignment |
| Offsets | `u32[N + 1]` adjacent starts/ends | Direct | Not represented |
| Lengths16 | `u32[ceil(N / 64)]` checkpoints, then `u16[N]` coded lengths | One checkpoint plus at most 63 earlier lengths | Derived from shared alignment |
| Lengths32 | `u32[ceil(N / 64)]` checkpoints, then `u32[N]` coded lengths | One checkpoint plus at most 63 earlier lengths | Derived from shared alignment |

All fields are little-endian and read bytewise; the table address need not be aligned. Checkpoints are physical starts of units 0, 64, 128 and so on. The first is zero. Every unit starts at `align_up(previous_end, alignment)`. Its end is that start plus its actual coded length. Count and input alignment come from shared group metadata and are not repeated in the index body.

For coded lengths `[3, 5, 2]` and alignment `64`:

```text
DATA 0..3      unit 0: 3 coded bytes
     3..64     alignment gap
     64..69    unit 1: 5 coded bytes
     69..128   alignment gap
     128..130  unit 2: 2 coded bytes
```

The complete span is 130 bytes. Decoder inputs have lengths 3, 5 and 2; none receives the gaps. DATA integrity coverage still includes those physical gaps. Independent decodability, sample coverage and reference availability are separate group/profile checks.

## Semantic construction

```rust
use mirx::{image::{UnitIndex, UnitSelection}, types::ByteAlignment};

let alignment = ByteAlignment::new(64).unwrap();
let ranges = [0..3, 64..69, 128..130];
let index = UnitIndex::ranges(&ranges, alignment).unwrap();
let cells = [0, 2, 5];
let selection = UnitSelection::cells(6, &cells).unwrap();
assert_eq!(index.get(1), Some(64..69));
assert_eq!(index.byte_len(), 130);
assert!(index.iter().rev().eq([128..130, 64..69, 0..3]));
assert!(selection.iter().eq([0, 2, 5]));
```

`UnitIndex::fixed(count, unit_bytes, alignment)` derives equal ranges. `UnitIndex::ranges` borrows ordinary Rust ranges and validates canonical aligned starts. `UnitSelection::cells` borrows ordered grid-cell ordinals. These forms are suitable for runtime composition and typed authoring without materializing MIRX wire tables.

## Stored forms

Typed IMAGE and FRAMES readers open stored index and selection tables internally, checking every checkpoint, length, total bound and alignment. They expose validated `UnitIndex` and `UnitSelection` values rather than raw table constructors. Forward and reverse iteration advances in constant time per adjacent item; skips use bounded lookup without expanded arrays or scans over skipped units.

`EncodedImageAsset::from_groups` and `FramesAsset::new` select the smallest canonical representation from semantic groups. Full selections and fixed coded lengths omit their tables. Sparse selections choose a sorted list or checkpointed bitmap; variable lengths choose adjacent offsets, checkpointed `u16` lengths or checkpointed `u32` lengths. Authoring rejects overflow and noncanonical ranges before writing output.

Stored group metadata uses index mode 0 for fixed, 1 for offsets, 2 for Lengths16 and 3 for Lengths32. Fixed coded length is uniquely recovered from DATA span, unit count and shared alignment. The last actual end is authoritative: no next start is calculated after the last unit, so an otherwise valid range ending at `u32::MAX` remains representable. Empty generic ranges are supported by the semantic index, but nonempty encoded IMAGE units reject them.
