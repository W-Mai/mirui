# Encoded glyph regions

`GlyphSurfaceRecord::encoded_glyphs(media, map)` binds scalar glyph geometry to referenced CODINGS, UNIT_GROUPS, UNIT_INDEX and DATA entries. It returns `font::EncodedGlyphs`, reusing static-image coverage, coding and exact-region reconstruction without nested IMAGE payloads or per-glyph coding fields.

`group_count()` reports caller workspace slots. `groups_into(slots, budget)` prepares those groups and returns `GlyphGroups` tied to the same map. `decode_plan(glyph_index, requirements, limits)` resolves the exact glyph region and returns an `ImageDecodePlan`; `decode_into(output, workspace)` then writes to caller storage. Neither preparation nor execution allocates decoded buffers.

```rust
use mirx::{PayloadLimits, coding::Rle,
    font::{GlyphMap, GlyphPacking, GlyphSurfaceRecord},
    image::{ColorDescription, CoverageBudget, EncodedImageAsset, SampleLayout,
        SurfaceDescriptor, SurfaceRequirements}, media::MediaPayload};

let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]).encode().unwrap();
let media = MediaPayload::open(&bytes).unwrap();
let record = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 2, 2).unwrap()
    .with_codings(1).unwrap();
let glyphs = record.encoded_glyphs(media, GlyphMap::glyph_major(2, 2, 2).unwrap()).unwrap();
let mut slots = [None];
let groups = glyphs.groups_into(&mut slots, &mut CoverageBudget::new(100)).unwrap();
let plan = groups.decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED).unwrap();
assert_eq!(plan.memory_plan().byte_len(), 4);
assert_eq!(plan.workspace_requirements().byte_len(), 8);
let mut output = [0; 4];
let mut workspace = [0; 8];
let decoded = plan.decode_into(&mut output, &mut workspace).unwrap();
assert_eq!(decoded.plane(0).unwrap().bytes(), &[42; 4]);
```

This example uses an existing media payload to demonstrate explicit section references. The surface record is not an embedded IMAGE chunk. A single whole-surface stream requires whole-surface staging even for one glyph; independent per-glyph or tile groups reduce workspace to the largest intersecting unit. Atlas crops preserve sub-byte origins and empty mapped regions. An empty glyph selects no units and needs no input, checksum or workspace bytes.

`with_file_offset(offset)` supplies the outer payload location for file/Flash alignment checks during preparation. `input_alignment()` reports a declaration, not an actual pointer guarantee. Output base, plane alignment and row stride are planned independently through `SurfaceRequirements`; binding errors leave output unchanged.

Opening encoded glyph metadata does not prove codec support or DATA integrity. `preflight(limits)` checks the complete group and all DATA coverage, with an explicit glyph-count cap. A selected glyph plan checks only required units and their integrity scope; indexed partitions can exclude unrelated corruption, while whole-DATA integrity still requires every DATA body. Unsupported profiles fail before execution. A plan copies its selected region and does not retain the glyph-map metadata lifetime.
