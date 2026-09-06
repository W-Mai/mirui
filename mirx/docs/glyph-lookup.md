# Joined glyph lookup

`font::GlyphTable` joins one shared Unicode directory, one representation's metrics and its prepared RAW glyph storage. Construction requires equal codepoint, metric and glyph counts before lookup. Validating each source independently is not enough: otherwise an ordinal can select unrelated metrics or samples.

`get(ordinal)` performs constant-time paired lookup. `glyph(char)` uses the shared Unicode binary search and returns `None` for missing characters. `line_metrics()` and each glyph's metrics come from the same representation table, in design-ppem units; no implicit scaling or fallback occurs. Coverage/SDF semantics and size selection belong to the containing representation, not this table.

The returned `Glyph` contains copied `GlyphMetrics` and a `GlyphRaster` borrowing only sample bytes. It can outlive codepoint, metric and map metadata. The view neither creates a merged record array nor allocates samples. Empty tables are valid components; complete face validation determines whether a font is usable. This API does not parse a FONT envelope or verify DATA integrity.

```rust
use mirx::{Fixed, font::{
    FontCodepoints, GlyphMap, GlyphMetrics, GlyphTable, LineMetrics, MetricsTable, RawGlyphs,
}, image::{SampleLayout, SurfaceRequirements}};

let codepoints = FontCodepoints::open(&[b'A', 0, 0, 0]).unwrap();
let line = LineMetrics::new(Fixed::from_int(3), Fixed::from_int(-1), Fixed::from_int(4)).unwrap();
let metric = GlyphMetrics::new(Fixed::from_ratio(5, 2), Fixed::from_ratio(-1, 4), Fixed::from_int(3));
let mut records = [0; 24];
line.encode_record_into(&mut records).unwrap();
metric.encode_record_into(&mut records[12..]).unwrap();
let metrics = MetricsTable::open(&records).unwrap();
let map = GlyphMap::glyph_major(2, 3, 1).unwrap();
let glyphs = RawGlyphs::builder(map, SampleLayout::A8).build(&[128; 6]).unwrap();
let table = GlyphTable::new(codepoints, metrics, glyphs).unwrap();
assert_eq!(table.line_metrics(), line);
assert!(table.glyph('B').is_none());
let glyph = table.glyph('A').unwrap();
assert_eq!(glyph.metrics(), metric);
let plan = glyph.raster().memory_plan(SurfaceRequirements::new()).unwrap();
let mut output = [0; 6];
glyph.raster().copy_into(&mut output, plan).unwrap();
assert_eq!(output, [128; 6]);
```

Storage coordinates and raster extents come from [glyph storage](glyph-storage.md); pen-relative placement comes from [glyph metrics](font-metrics.md). Neither source repeats Unicode values. A missing glyph is distinct from a present glyph with an empty raster region and a nonzero advance.
