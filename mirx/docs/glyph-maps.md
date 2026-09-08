# Glyph region maps

`font::GlyphMap` maps raster ordinals to logical raster regions. It contains either derived fixed cells or a validated `image::AtlasMap`; font code does not define another atlas record format.

## Two packings

| Packing | Region for ordinal `i` | Map bytes |
| --- | --- | --- |
| `GlyphMajor` | `(0, i × cell_height, cell_width, cell_height)` | None |
| `Atlas2D` | Region from a shared `AtlasMap` | 16 per glyph in `ATLAS_MAPS` |

GlyphMajor uses a vertical logical surface with width `cell_width` and height `glyph_count × cell_height`. Cells must be nonempty; count zero yields logical height zero. Checked arithmetic rejects coordinate overflow. Geometry reuses `image::TileGrid`; no expanded region array is created.

Atlas2D validation and wire encoding belong to [`image::AtlasMap`](atlas-map.md). `GlyphMap` only binds that shared geometry to raster ordinals.

Packing is explicit, not inferred from map length. An empty Atlas2D table is not a GlyphMajor table. The complete face validates map cardinality against `FACE.raster_count`.

## Construct

```rust
use mirx::{font::{GlyphMap, GlyphPacking}, image::{AtlasMap, Region}};

let cells = GlyphMap::cells(7, 9, 3)?;
assert_eq!(cells.packing(), GlyphPacking::GlyphMajor);
assert_eq!(cells.get(2), Region::new(0, 18, 7, 9).ok());

let regions = [
    Region::new(1, 2, 5, 7).unwrap(),
    Region::new(0, 0, 0, 0).unwrap(),
];
let atlas = AtlasMap::new(16, 16, &regions).unwrap();
let glyphs = GlyphMap::atlas(atlas);
assert_eq!(glyphs.get(0), Some(regions[0]));
assert_eq!(glyphs.iter().last(), Some(regions[1]));
# Ok::<(), mirx::font::GlyphMapError>(())
```

Indexed lookup, forward/backward skips and implicit-map construction take constant time. Iterators are exact-size and double-ended. Implicit cells require no map bytes.

## Logical regions are not memory offsets

The map contains no Unicode scalars, glyph IDs, placement, coding IDs, DATA offsets, stride or alignment. Coordinates remain samples, including sub-byte x positions. Physical padding and per-glyph allocation gaps must be resolved through storage layout; a vertical logical GlyphMajor surface does not imply one contiguous physical plane.

A mapped glyph need not equal one decode unit. Whole-atlas coding can require a shared decoded surface; tiled coding can require multiple units for one rectangle. The map itself performs neither decoding nor unit selection. Identity and placement use the [FONT payload structure](font-payload.md); sample storage and caller output remain separate contracts.

`font::RawGlyphs` binds either map to scalar RAW samples using shared physical plane rules. Its [borrowed storage contract](glyph-storage.md) accounts for cell allocation rows, alignment gaps and exact atlas regions without treating them as one implicit contiguous bitmap.
