# RLE coding

`CodingId::RLE` (`2`), revision `1`, stores independent streams of fixed-width byte elements. Empty parameters select one-byte elements. A single parameter byte `2`, `3` or `4` selects that element width. Explicit `1`, unsupported widths and additional parameter bytes are invalid. Element size does not imply a color format or a row boundary.

| Control bits | Count | Body |
| --- | --- | --- |
| `0 nnnnnnn` | `n + 1` elements | That many literal elements |
| `1 nnnnnnn` | `n + 1` elements | One element, repeated that many times |

Counts are always 1–128. There are no zero-length tokens, end markers, dictionaries, cross-unit history or size prefixes. The caller supplies the exact decoded byte count, which must be divisible by element size. Tokens must consume the complete input and produce exactly that count. Byte values are retained without conversion.

Canonical encoding compares two greedy tokenizations, using minimum repeat lengths of 2 and 3. Each splits literals and runs at 128 elements. The smaller complete stream is emitted; ties select repeat threshold 2. Threshold is an encoder choice, not a wire parameter. Exact counting and emission share the same token generator and need no intermediate encoded buffer. The conservative upper bound is input byte count plus element count; short runs can split literal packets, so the all-literal overhead is not a universal bound.

`coding::Rle::new()` selects byte RLE; `with_element_size` validates other widths. `encoded_len` and `encode_into` count before writing, reject partial elements and leave output unchanged on errors. `plan(input, decoded_len)` validates token lengths, bounds and exact consumption without allocating or writing output. `RleDecodePlan::decode_into` checks capacity before replaying immutable input, preserves the suffix and cannot partially write an error result.

The codec describes tight byte streams. Surface stride, sample packing, palettes, color interpretation, checksums, frame composition and tool selection are separate contracts. Compression can expand input; asset selection must compare complete encoded and RAW storage sizes.

## Plane output

RLE `DecodeUnitRef` values decode selected tight plane rows in original plane-index order. A plane contributes `minimum_stride() × height` bytes, excluding physical stride/allocation padding. Element grouping continues across rows and planes; the total byte count must be divisible by element size. Packed/indexed/alpha and planar YUV layouts use the same byte mapping without color conversion or synthesized palettes.

`decode_plan` validates the entire stream and target layout before returning a plan. Execution maps literals and repeated elements directly into caller-aligned plane rows; it does not allocate a tight intermediate image. Physical padding is zero, and unused low bits in sub-byte row tails are cleared after decoding. Logical samples remain exact; non-sample tail bits are canonicalized consistently with RAW transfer. Chroma-only and sub-byte units retain their original plane indices and source regions. Source integrity remains a separate media check.
