# Joined face tables

`font::FaceTables` binds shared codepoints and representation records to exact METRICS and GLYPH_MAPS bodies. `select(request)` returns one `FaceRepresentation` containing the selected size record, hinted line/glyph metrics, shared surface record and glyph map. Selection and glyph access allocate nothing and do not rescan atlas records.

For `N` shared codepoints and `R` representations, METRICS stores `R × (12 + 12 × N)` bytes. Each representation has its own line prefix followed by metrics in Unicode ordinal order. A glyph's dimensions come from its map, not its metrics. Selecting another representation changes both geometry and measurements together.

GLYPH_MAPS contains complete `16 × N` byte atlas tables. An Atlas2D representation can reference one complete table shared with another representation; each binding validates that table against its own surface dimensions. Partial aliases, unreferenced tables and out-of-bounds regions are errors. GlyphMajor stores no map bytes and requires offset zero. Empty character tables require empty maps and zero offsets.

`FaceTables::new(codepoints, representations, metrics, maps)` checks matching character counts, exact body lengths, every line prefix and every referenced atlas region. Resource limits come from the already validated `RepresentationTable`. `get(index)` then resolves only that representation's constant-size metadata, without rescanning its glyph map. `select(request)` uses the same ranking as `FontRepresentations`; `used_fallback()` reports an out-of-range nearest selection.

```rust
use mirx::{Fixed, FontRepresentation, FontRepresentationRequest, PayloadLimits,
    font::{FaceTables, FontCodepoints, GlyphPacking, GlyphSurfaceRecord,
        LineMetrics, RepresentationRecord, RepresentationTable}, image::SampleLayout};

let codepoint_bytes = ('A' as u32).to_le_bytes();
let codepoints = FontCodepoints::open(&codepoint_bytes).unwrap();
let mut surface = [0; 24];
GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 8, 8, 0).unwrap()
    .encode_record_into(&mut surface).unwrap();
let mut record = [0; 16];
RepresentationRecord::new(FontRepresentation::coverage(8, 12, 64).unwrap(), 0)
    .encode_record_into(&mut record).unwrap();
let representations = RepresentationTable::open(&record, &surface, 1, &PayloadLimits::EMBEDDED).unwrap();
let mut metrics = [0; 24];
LineMetrics::new(Fixed::from_int(10), Fixed::from_int(-2), Fixed::from_int(12)).unwrap()
    .encode_record_into(&mut metrics).unwrap();
let tables = FaceTables::new(codepoints, representations, &metrics, &[]).unwrap();
let selected = tables.select(FontRepresentationRequest::new(12)).unwrap();
assert_eq!(selected.index(), 0);
assert!(!selected.used_fallback());
assert_eq!(selected.metrics().len(), 1);
assert_eq!(selected.map().get(0).unwrap().width(), 8);
```

These are metadata tables, not nested FONT or IMAGE payloads. The selected `surface()` record binds RAW or encoded samples separately through `raw_glyphs(media, map)` or `encoded_glyphs(media, map)`. Media-directory rules, storage admission, coding preflight and DATA integrity are separate from this metadata join.
