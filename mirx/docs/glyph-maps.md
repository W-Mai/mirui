# Glyph region maps

`font::GlyphMap` maps raster ordinals to logical raster regions. It accepts implicit fixed cells, borrowed native `Region` values, or borrowed little-endian atlas records. Every path exposes the same checked lookup, iteration and caller-buffer encoding API without allocation.

## Two packings

| Packing | Region for ordinal `i` | Map bytes |
| --- | --- | --- |
| `GlyphMajor` | `(0, i × cell_height, cell_width, cell_height)` | None |
| `Atlas2D` | Explicit `(x, y, width, height)` | 16 per glyph |

GlyphMajor uses a vertical logical surface with width `cell_width` and height `glyph_count × cell_height`. Cells must be nonempty; count zero yields logical height zero. Checked arithmetic rejects coordinate overflow. Geometry reuses `image::TileGrid`; no expanded region array is created.

Atlas2D records are four little-endian `u32` fields: x at byte 0, y at byte 4, width at byte 8 and height at byte 12. The byte length derives the record count without a table header. Every exclusive right/bottom edge must fit the shared atlas extent. The all-zero record is the only empty-glyph encoding; any other zero width or height is rejected. Shared and overlapping nonempty rectangles are valid because map entries reference samples rather than perform writes.

Packing is explicit, not inferred from map length. An empty Atlas2D table is not a GlyphMajor table. The complete face validates map cardinality against `FACE.raster_count`.

## Construct, encode and read

```rust
use mirx::{font::{GlyphMap, GlyphPacking}, image::Region};

let cells = GlyphMap::glyph_major(7, 9, 3)?;
assert_eq!(cells.packing(), GlyphPacking::GlyphMajor);
assert_eq!(cells.encoded_len(), 0);
assert_eq!(cells.get(2), Region::new(0, 18, 7, 9).ok());
assert_eq!(cells.encode_into(&mut [])?, 0);

let regions = [
    Region::new(1, 2, 5, 7).unwrap(),
    Region::new(0, 0, 0, 0).unwrap(), // No raster samples.
];
let atlas = GlyphMap::atlas(16, 16, &regions)?;
let mut bytes = [0; 32];
atlas.encode_into(&mut bytes)?;
let borrowed = GlyphMap::from_records(16, 16, &bytes)?;
assert_eq!(borrowed.get(0), Some(regions[0]));
assert_eq!(borrowed.iter().last(), Some(regions[1]));
# Ok::<(), mirx::font::GlyphMapError>(())
```

Native and wire atlas validation scan records once, after checking the table's representable byte span. Indexed lookup, forward/backward skips and implicit-map construction take constant time. Iterators are exact-size and double-ended. Encoding checks output capacity before writing, preserves the suffix and emits no bytes for implicit maps.

## Logical regions are not memory offsets

The map contains no Unicode scalars, glyph IDs, placement, coding IDs, DATA offsets, stride or alignment. Coordinates remain samples, including sub-byte x positions. Physical padding and per-glyph allocation gaps must be resolved through storage layout; a vertical logical GlyphMajor surface does not imply one contiguous physical plane.

A mapped glyph need not equal one decode unit. Whole-atlas coding can require a shared decoded surface; tiled coding can require multiple units for one rectangle. The map itself performs neither decoding nor unit selection. Identity and placement use the [FONT payload structure](font-payload.md); sample storage and caller output remain separate contracts.

`font::RawGlyphs` binds either map to scalar RAW samples using shared physical plane rules. Its [borrowed storage contract](glyph-storage.md) accounts for cell allocation rows, alignment gaps and exact atlas regions without treating them as one implicit contiguous bitmap.
