# Glyph surface records

`font::GlyphSurfaceRecord` stores shared sample geometry and directory references in 24 bytes. Section offsets and lengths remain in the common media directory; glyphs do not repeat them. RAW allocation and encoded input are mutually exclusive storage states.

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

`with_planes` rejects encoded storage; `with_codings` rejects a RAW allocation. `with_groups(section, index_section)` requires coding and attaches an optional index together with its owning groups. Native calls cannot pass the absent-reference sentinel as a real ordinal. Parsing enforces the same constraints and rejects unknown packing or nonzero reserved bytes.

```rust
use mirx::font::{GlyphPacking, GlyphSurfaceRecord, GLYPH_SURFACE_RECORD_LEN};
use mirx::image::SampleLayout;

let record = GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 12, 16, 7)?
    .with_codings(8)?.with_groups(9, Some(10))?;
assert_eq!(record.logical_extent(96)?, (12, 1536));
let mut bytes = [0; GLYPH_SURFACE_RECORD_LEN];
record.encode_record_into(&mut bytes)?;
assert_eq!(GlyphSurfaceRecord::from_record(&bytes)?, record);
# Ok::<(), mirx::font::GlyphSurfaceRecordError>(())
```

`validate_sections(media)` checks direct directory ordinals, expected kinds and exact REQUIRED flags in constant space. It does not establish body validity, complete face cardinality, coding support, coverage or DATA integrity. Referenced immutable sections may be shared by several records. RAW binding requires one exact 24-byte PLANES record when present; implicit encoded storage requires exactly one coding record when groups are absent.

Record reads and writes allocate nothing and accept unaligned input. Parsing consumes a 24-byte prefix; complete tables must have an exact multiple of that size. Short output errors preserve all bytes, and successful writes leave the suffix unchanged.
