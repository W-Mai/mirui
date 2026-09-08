# FONT payload structure

A FONT payload contains one logical face and one or more raster representations. Glyph identity is independent of Unicode, raster order, placement, and sample storage.

```text
FONT MediaPayload
├─ FACE                 one face header and design-space line metrics
├─ CMAP_INDEX           sorted Unicode scalar → GlyphId records
├─ GLYPH_IDS?           raster ordinal → GlyphId, omitted for 0..N identity order
├─ ADVANCES | SHAPING   exactly one placement source
├─ REPRESENTATIONS      coverage, signed-distance, or application records
├─ RASTER_METRICS       representation-major raster offsets
├─ ATLAS_MAPS?          Atlas2D regions, omitted for derived GlyphMajor cells
├─ SURFACE_GROUPS       shared scalar surface records
├─ storage metadata     PLANES, CODINGS, UNIT_GROUPS, UNIT_INDEX, INTEGRITY
└─ DATA                 RAW or coded sample bytes
```

`FACE.raster_count` defines `N`. `CMAP_INDEX` may contain fewer or more entries than `N` because multiple scalars can map to one glyph and shaping-only glyphs need no scalar mapping. Every mapped and default glyph must resolve through the identity or sparse raster table.

`ADVANCES` contains `N` signed 24.8 values in face design units. `SHAPING` contains a bounded SFNT subset with cmap, horizontal metrics, shaping tables, and outlines; it replaces `ADVANCES` when runtime shaping owns placement. Storing both is invalid. Every effective Unicode mapping in the shaping face must match `CMAP_INDEX` in both directions.

`RASTER_METRICS` contains `representation_count × N` pairs of signed 24.8 offsets in design-ppem pixels. Empty glyphs retain identity, advance, and offsets while their map region has zero width or height.

Each representation references one `SURFACE_GROUPS` record. Several representations may share a surface. GlyphMajor maps derive fixed cells from surface geometry; Atlas2D maps reference one explicit `N × 16` byte region table.
