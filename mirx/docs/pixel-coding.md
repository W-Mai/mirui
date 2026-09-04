# Pixel coding

`CodingId::PIXEL` (`1`), revision `1`, stores independent lossless RGB888 or RGBA8888 sample streams. Parameters are empty. Geometry, sample layout, color description, input range and integrity belong to the containing media records. The stream has no file signature, size prefix or footer.

Each unit starts with previous pixel `(0, 0, 0, 255)` and a 64-entry RGBA cache filled with `(0, 0, 0, 0)`. RGB samples have implicit alpha 255. A cache reference containing any other alpha is invalid in RGB mode.

| Byte range | Operation | Additional bytes |
| --- | --- | --- |
| `00..3d` | Repeat the previous pixel `byte + 1` times (1–62) | None |
| `3e` | Replace RGB, retaining previous alpha | R, G, B |
| `3f` | Replace RGBA; RGBA8888 only | R, G, B, A |
| `40..7f` | Read cache slot `byte & 63` | None |
| `80..bf` | Three 2-bit RGB deltas, each biased by 2 | None |
| `c0..ff` | 6-bit green delta biased by 32 | High nibble: R−G delta; low nibble: B−G delta; both biased by 8 |

The small-delta byte has bits `10 rr gg bb`. The green-delta pair has bits `11 gggggg rrrr bbbb`: add `g−32` to green, `g−32+r−8` to red, and `g−32+b−8` to blue. All additions wrap modulo 256; alpha is unchanged. Deltas apply to the previous pixel, including across row boundaries within one unit.

After each output pixel, update the previous value and cache slot `(R×3 + G×5 + B×7 + A×11) mod 64`. Runs perform the same update; initial previous-pixel runs therefore seed the opaque-black cache entry. An independently indexed unit resets both state values.

Canonical encoding chooses a previous-pixel run before cache lookup, then an alpha-changing literal, small RGB delta, green delta, or RGB literal. Runs stop at 62 or a differing pixel. Signed modular differences use `−128..127`; green residual differences are modular too. The upper bound is 4 bytes per RGB pixel or 5 bytes per RGBA pixel. Literal fallback bounds expansion but does not guarantee compression; asset selection must compare complete encoded and RAW sizes.

`coding::Pixel::plan` validates the exact requested pixel count and complete input consumption before returning `PixelDecodePlan`. Truncated literals, excess runs, trailing bytes and invalid RGB alpha fail during planning. Planning processes a run as one token; it does not allocate or materialize pixels. `decode_into` checks output capacity before replaying the validated immutable input. It returns a tight decoded prefix, preserves the suffix and leaves all output unchanged on error. `encoded_len` and `encode_into` share canonical token generation, with the same no-allocation and failure-before-writes contract.

This byte-level API does not treat stride padding as samples, accept alignment bytes after a stream, verify media CRC, perform color conversion or reconstruct a previous frame. Input/output alignment, unit placement, strided surface writes and checksum validation are separate contracts. The stream is not a standard QOI file.
