# Borrowed font representation tables

`font::RepresentationTable` borrows exact REPRESENTATIONS and SURFACE_GROUPS bodies. Each representation resolves its scalar surface by ordinal, derives sample depth and tight decoded cost, and uses the same duplicate-identity and size-selection rules as native `FontRepresentations`. No decoded metadata array is allocated.

Representation count comes from 20-byte records; referenced surfaces use 24-byte records. GlyphMajor logical height derives from shared glyph count, while Atlas2D keeps its stored extent. Surface index 65535 is valid when 65536 surface records are present: this index has no absent sentinel and is not a common-directory reference.

| Limit | Embedded | Host | Scope |
| --- | ---: | ---: | --- |
| `max_font_representations` | 64 | 1024 | Record parsing and pairwise duplicate checks |
| `max_font_glyphs` | 4096 | 1,000,000 | Shared glyph cardinality before geometry |

Count limits are enforced before per-record parsing or duplicate comparisons. Zero disables the corresponding resource. Tight decoded cost is selection metadata, not a requested allocation; `max_decoded_bytes` therefore does not reject a borrowed atlas at this step. Execution applies its output, unit and work limits independently.

`FontView::representations` returns the admitted table. `get`, iteration and `select` operate on that borrowed view without exposing record serialization or section ordinals to authoring code.

`get` and direct iterator skips resolve only the selected record in constant time. Iteration is exact-size, fused and double-ended. `select` returns an inline `FontRepresentationMatch`; neither that result nor an individual record borrows the source table. Requests, exact Coverage preference, SDF intervals and fallback retain the native selector's semantics.

The typed table supports scalar Coverage/SDF and scalar application-defined records. Other application storage remains available through raw payload preservation, without invented color semantics. Referenced surface records are checked; unused surface records are not interpreted by this representation-only view. Glyph identity, placement, atlas-map references, storage section references, unused sections and DATA integrity remain complete-face responsibilities. Opening this table alone does not validate a FONT payload.
