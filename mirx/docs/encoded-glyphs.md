# Encoded glyph regions

`FontView::glyphs(representation_index)` binds scalar glyph geometry to the selected representation's CODINGS, UNIT_GROUPS, UNIT_INDEX and DATA sections. An encoded representation returns `FontGlyphs::Encoded`, reusing static-image coverage, coding and exact-region reconstruction without nested IMAGE payloads or per-glyph coding fields.

`group_count()` reports caller workspace slots. `groups_into(slots, budget)` prepares those groups and returns `GlyphGroups` tied to the same map. `decode_plan(glyph_index, requirements, limits)` resolves the exact glyph region and returns an `ImageDecodePlan`; `decode_into(output, workspace)` then writes to caller storage. Neither preparation nor execution allocates decoded buffers.

A single whole-surface stream requires whole-surface staging even for one glyph; independent per-glyph or tile groups reduce workspace to the largest intersecting unit. Atlas crops preserve sub-byte origins and empty mapped regions. An empty glyph selects no units and needs no input, checksum or workspace bytes.

`with_file_offset(offset)` supplies the outer payload location for file/Flash alignment checks during preparation. `input_alignment()` reports a declaration, not an actual pointer guarantee. Output base, plane alignment and row stride are planned independently through `SurfaceRequirements`; binding errors leave output unchanged.

Opening encoded glyph metadata does not prove codec support or DATA integrity. `preflight(limits)` checks the complete group and all DATA coverage, with an explicit glyph-count cap. A selected glyph plan checks only required units and their integrity scope; indexed partitions can exclude unrelated corruption, while whole-DATA integrity still requires every DATA body. Unsupported profiles fail before execution. A plan copies its selected region and does not retain the glyph-map metadata lifetime.
