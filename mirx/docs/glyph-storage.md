# Borrowed glyph storage

`font::RawGlyphs` binds a validated `GlyphMap` to borrowed A1/A2/A4/A8 samples. The representation determines whether samples mean coverage or signed distance. Storage uses the shared image geometry, physical plane layout, row access and cropped transfer; it does not decode a FONT envelope or verify its checksums.

| Packing | Physical allocation | Glyph lookup |
| --- | --- | --- |
| GlyphMajor | One shared cell layout, repeated at aligned addresses | Derive cell offset from ordinal; region starts at `(0, 0)` in that cell |
| Atlas2D | One shared atlas plane | Borrow the atlas and retain the mapped region, including sub-byte origins |

`RawGlyphs::builder(map, layout).build(data)` uses tight defaults. `with_memory_layout` supplies one `PlaneMemoryLayout` for a cell or the whole atlas. GlyphMajor cell spacing is the physical cell byte length rounded up to the declared alignment. Initial offset, row padding and allocation-only rows remain separate from the glyph's logical dimensions. The last cell has no trailing alignment gap. Zero cells require no bytes; an atlas with no mapped glyphs may still contain its atlas allocation. DATA must match the exact derived span.

`get(ordinal)` resolves in constant time and returns a `GlyphRaster` borrowing only sample bytes, not map metadata. `storage()` exposes the backing `SurfaceView`; `region()` locates the glyph within it. `memory_plan(requirements)` and `copy_into(output, plan)` reuse exact image cropping and caller-buffer address checks. Successful output has zero padding and preserves its unused suffix. A plan for another source or region fails before writes. No lookup, planning or transfer allocates.

```rust
use mirx::{ByteAlignment, font::{GlyphMap, RawGlyphs}, image::{
    ColorDescription, PlaneMemoryLayout, SampleLayout, SurfaceDescriptor, SurfaceRequirements,
}};

let map = GlyphMap::cells(5, 3, 2).unwrap();
let cell = SurfaceDescriptor::new(5, 3, SampleLayout::A4, ColorDescription::NONE).unwrap();
let memory = PlaneMemoryLayout::builder(cell.plane(0).unwrap())
    .with_stride(64)
    .with_alignment(ByteAlignment::new(64).unwrap())
    .build()
    .unwrap();
#[repr(align(64))]
struct Source([u8; 384]);
let source = Source([0x12; 384]);
let glyphs = RawGlyphs::builder(map, SampleLayout::A4)
    .with_memory_layout(memory).build(&source.0).unwrap();
assert!(glyphs.data_addresses_are_aligned());
let glyph = glyphs.get(1).unwrap();
assert_eq!(glyph.storage().plane(0).unwrap().memory().data_offset(), 192);
let plan = glyph.memory_plan(SurfaceRequirements::new()).unwrap();
let mut output = [0; 9];
let view = glyph.copy_into(&mut output, plan).unwrap();
assert_eq!(view.plane(0).unwrap().row(0).unwrap(), Some(&[0x12, 0x12, 0x10][..]));
```

`file_address_is_aligned(data_offset)` checks declared placement; `data_addresses_are_aligned()` checks actual borrowed storage. Neither implies the other. Misaligned source slices can still use CPU reads and explicit transfer. Atlas-base alignment does not imply every glyph's interior origin is aligned or byte-addressable. Unknown physical flags remain representable, but linear row access and copying reject unsupported flags. Compressed bytes cannot be passed as RAW cells, and physical padding is never codec input.

[`FontView`](font-payload.md) binds this storage to glyph identity, placement, and representation records, rejecting cardinality mismatches before lookup.
