# Glyph surface records

`font::GlyphSurfaceRecord` is the read-only typed projection of shared sample geometry and directory references stored in 24 bytes. Section offsets and lengths remain in the common media directory; glyphs do not repeat them. RAW allocation and encoded input are mutually exclusive storage states. `GlyphSurfaceAsset` supplies semantic storage while the writer assigns directory ordinals.

| Bytes | Field | Meaning |
| --- | --- | --- |
|0–1|sample_layout:u16|Shared sample arrangement|
|2|packing:u8|0 GlyphMajor, 1 Atlas2D|
|3|reserved:u8|Zero|
|4–7|width:u32|Cell width or atlas width|
|8–11|height:u32|Cell height or atlas height|
|12–13|data_section:u16|Required DATA directory ordinal|
|14–15|planes_section:u16|Optional shared RAW allocation|
|16–17|codings_section:u16|Optional shared coding table|
|18–19|groups_section:u16|Optional encoded unit groups|
|20–21|index_section:u16|Optional unit index|
|22–23|reserved:u16|Zero|

Fields are little-endian. Optional references encode absence as 65535, which cannot address an entry in a directory whose count is `u16`. DATA must have a real ordinal. Unknown sample-layout identifiers remain representable; interpretation requires a supported sample contract.

GlyphMajor stores positive cell dimensions and derives logical height from shared glyph count through `GlyphMap`. Atlas2D uses its full extent, including empty dimensions, independently of glyph count. Physical allocation, row stride, cell alignment gaps and actual source addresses are separate from these logical dimensions.

```text
GlyphSurfaceRecord
├─ geometry: sample layout + packing + cell/atlas dimensions
├─ DATA ordinal ──────────────────► directory entry ─► sample bytes
└─ one storage state
   ├─ RAW: optional PLANES ────────► shared physical allocation
   └─ encoded: CODINGS ────────────► coding table
               UNIT_GROUPS? ──────► shared unit rules
               UNIT_INDEX? ───────► group indexes
```

The writer rejects a physical allocation on encoded storage and rejects coding or group references on RAW storage. Native authoring never receives directory ordinals or the absent-reference sentinel. Parsing enforces the same constraints and rejects unknown packing or nonzero reserved bytes.

```rust
use mirx::font::{GlyphMap, GlyphSurfaceAsset, RawGlyphs};
use mirx::image::SampleLayout;

let map = GlyphMap::cells(2, 2, 2).unwrap();
let samples = [0_u8; 4];
let glyphs = RawGlyphs::builder(map, SampleLayout::A4).build(&samples).unwrap();
let surface = GlyphSurfaceAsset::raw(glyphs);
assert_eq!(surface.map().cell_extent(), Some((2, 2)));
```

Complete `FontView` validation checks direct directory ordinals, expected kinds and exact REQUIRED flags in constant space. It then establishes body validity, complete face cardinality and storage binding without conflating those checks with DATA integrity. Referenced immutable sections may be shared by several records. RAW binding requires one exact 24-byte PLANES record when present; implicit encoded storage requires exactly one coding record when groups are absent.

`FontView::glyphs` binds matching scalar maps to the referenced RAW PLANES and DATA through `RawGlyphs`. It requires identical packing and cell/atlas dimensions, parses the exact physical record and checks the complete DATA span, including shared cell-alignment gaps. Encoded storage and unsupported scalar layouts are rejected. The returned storage borrows the original DATA and reuses existing glyph lookup and crop transfer.

This low-level binding does not checksum samples. Verify the complete face or selected DATA range before trusted consumption; repeated glyph binding must not silently rescan all representations. Source-address and file-address checks remain explicit on `RawGlyphs`. Unknown physical flags remain inspectable, while row access and sample copying reject unsupported storage interpretation.

Record reads allocate nothing and accept unaligned input. Complete tables must have an exact multiple of the stored record size. Serialization and section binding remain private to the checked FONT reader and writer.
