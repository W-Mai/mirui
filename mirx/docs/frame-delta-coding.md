# Frame-delta coding

`CodingId::FRAME_DELTA` (`6`), revision `1`, stores lossless byte residuals predicted from the same unit in the previous displayed frame. Parameters are empty. The surrounding FRAMES group supplies surface geometry, selected planes, unit range, integrity and `ReferenceMode::Previous`; those facts are not repeated in the stream.

Reconstruction uses `current = previous + residual` with modulo-256 arithmetic. Tight selected-plane rows are concatenated in plane-index order. Physical row padding, plane gaps, allocation extent and output alignment are target-memory properties and never enter prediction.

| Control | Meaning | Following bytes |
| --- | --- | --- |
| `0x00..=0x3f` | 1–64 zero residuals | none |
| `0x40..=0x7f` | 1–64 copies of one residual | one residual byte |
| `0x80..=0xbf` | 1–64 literal residuals | the residual bytes |
| `0xc0..=0xcf` | repeating pattern of 1–16 bytes | little-endian `repetitions - 2`, then the pattern |
| `0xd0..=0xff` | reserved | invalid |

A pattern token repeats between 2 and 65,537 times. It compacts interleaved channel changes such as an RGBA alpha fade without making the codec depend on a particular sample layout. The encoder compares repeat thresholds and pattern savings before emitting deterministic tokens. Worst-case literal storage is `decoded_len + ceil(decoded_len / 64)`.

`FrameDelta::encoded_len` and `encode_into` require equal predictor/current lengths and allocate nothing. `plan(input, decoded_len)` proves exact reconstruction length, complete input consumption and bounded pattern multiplication before output writes. `FrameDeltaDecodePlan::decode_into` accepts a tight predictor. A prepared unit uses `decode_from(reference, output)`: it copies the referenced unit from the retained canvas into reusable tight workspace, applies residuals in place, and expands the result into its caller-selected strided and aligned layout.

`FrameDeltaDecodePlan::apply_with` separates validated token parsing from residual execution. `FrameDeltaKernel` receives only complete output ranges and validated residual slices, so scalar, SIMD and device adapters share one wire definition. `plan` records whether the stream contains a vector-eligible block of at least 64 bytes. `apply_into` uses one private portable 16-byte implementation on x86-64, AArch64 and wasm32 when that work exists; short or unsupported streams retain `ScalarFrameDelta`. Literal and repeated residuals use complete vectors plus scalar tails. Patterns whose period divides 16 use one repeated lane; other periods retain the scalar implementation. No kernel requires aligned input/output addresses or heap storage.

The retained architecture-specific baseline measured 320×240 RGBA reconstruction including predictor copying and complete output observation. AArch64 constant four-byte patterns measured 21.82–24.64× scalar and incompressible literals measured 2.31–2.41×; token-fragmented scroll data remained within noise at 0.98–1.01×. The equivalent translated x86-64 process measured 19.85–20.74× for the constant pattern and 2.22–3.40× for literal-heavy streams. The portable AArch64 kernel, measured with the same frame size and complete reconstruction boundary, reached 7.92–8.39× for a four-byte RGBA pattern and 1.18–1.31× for literal-heavy residuals. Plan-level short-stream selection kept fragmented changes at 0.95–1.01× scalar. These measurements do not verify MVE, GPU, firmware or DMA paths.

Static IMAGE storage rejects previous-frame references. A FRAMES decoder session validates the complete recovery path, selected DATA integrity, canvas, unit workspace and disposal backup before changing the current frame. Seeking starts from a bounded recovery frame, so reference chains cannot silently turn random access into an unbounded scan.

No profile is always smallest. Frames with no changed groups should be omitted, localized changes may prefer sparse independently coded tiles, coherent dense changes may prefer frame-delta coding, and incompressible changes must use an independent RAW or other scalar profile. Selection belongs to asset generation and compares complete group, index and DATA sizes rather than the residual body alone.
