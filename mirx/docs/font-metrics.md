# Font metric records

`font::LineMetrics`, `GlyphMetrics` and `MetricsTable` describe the measurements for one raster representation. Records use signed 24.8 design-ppem pixels and little-endian `i32` storage. They are independent of atlas samples and byte coding; `MetricsTable` is a record-table reader, not a complete FONT payload parser.

```text
One representation's metric table
├─ LineMetrics             12 bytes, stored once
│  ├─ ascent               nonnegative
│  ├─ descent              nonpositive
│  └─ line_height          positive baseline advance
└─ GlyphMetrics[N]         12 bytes per shared glyph ordinal
   ├─ advance              horizontal pen advance
   ├─ bearing_x            raster left relative to pen x
   └─ bearing_y            raster top above baseline
```

## Byte layout

| Record | Bytes 0–3 | Bytes 4–7 | Bytes 8–11 |
| --- | --- | --- | --- |
| `LineMetrics` | `ascent` | `descent` | `line_height` |
| `GlyphMetrics` | `advance` | `bearing_x` | `bearing_y` |

Each value is a signed integer divided by 256: raw `384` means 1.5 pixels and raw `-128` means −0.5 pixels. Every complete glyph tuple is valid, including negative advances and bearings outside eight-bit limits. Line height may differ from ascent minus descent; it explicitly determines spacing between baselines.

Bearings locate the complete raster rectangle, including an SDF border. In a downward-pointing raster coordinate system, `left = pen_x + bearing_x` and `top = baseline - bearing_y`. A larger cell cannot use the bearing of an ink box inside that cell as its own raster origin. Scaling and placement must use checked arithmetic; record reading does not clamp values.

The table's length derives `N`: `(byte_len - 12) / 12`. There is no stored count. Glyph ordinal is supplied by the shared codepoint table, so metrics do not repeat Unicode values. Raster bounds belong to the surface or glyph map; stride, alignment and coding belong to shared storage, not each metric record.

## Borrowed access

```rust
use mirx::{Fixed, font::{GlyphMetrics, LineMetrics, MetricsTable}};

let line = LineMetrics::new(
    Fixed::from_int(12), Fixed::from_int(-4), Fixed::from_int(18),
)?;
let glyph = GlyphMetrics::new(
    Fixed::from_ratio(15, 2), // 7.5-pixel advance
    Fixed::from_ratio(-1, 2), // -0.5-pixel left bearing
    Fixed::from_int(11),
);
let mut bytes = [0; 24];
line.encode_record_into(&mut bytes)?;
glyph.encode_record_into(&mut bytes[12..])?;
let table = MetricsTable::open(&bytes)?;
assert_eq!(table.line_metrics(), line);
assert_eq!(table.get(0), Some(glyph));
assert_eq!(table.len(), 1);
# Ok::<(), mirx::font::MetricsError>(())
```

Opening validates the line prefix and exact record divisibility in constant time. A prefix with no glyph records is structurally valid; the complete face must validate cardinality against its codepoints. Reads decode fields directly from bytes, including unaligned addresses. Indexed lookup and iterator skips take constant time; iteration is exact-size and double-ended. None of these operations allocate.

Record encoders preserve the output suffix and leave short buffers unchanged. The serialized metric table has no address-alignment requirement; that does not relax alignment requirements on bulk glyph samples or decode output.
