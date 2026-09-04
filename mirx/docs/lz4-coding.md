# LZ4 coding

`CodingId::LZ4` (`3`), revision `1`, stores independent raw LZ4 blocks with empty parameters. There is no frame, size prefix, external dictionary or cross-unit history. The caller supplies the exact decoded byte count. Media integrity, sample layout and physical memory layout are separate contracts.

## Sequence structure

| Field | Encoding | Meaning |
| --- | --- | --- |
| Token | 1 byte: `LLLL MMMM` | Literal length and match length minus 4 |
| Literal extension | Present when `LLLL = 15` | Add bytes until a value below 255 terminates the extension |
| Literals | Literal length bytes | Copy unchanged |
| Offset | Little-endian `u16`; absent in final sequence | Nonzero backward distance into already decoded bytes |
| Match extension | Present when `MMMM = 15`; absent in final sequence | Add bytes to `4 + MMMM` until a value below 255 |

A saturated nibble requires an extension byte even when the extension is zero. Matches may overlap their own output; a distance of 1 repeats the preceding byte. Distance cannot exceed bytes already produced in the current block. No match refers to another block or frame.

The final sequence contains literals only and uses `MMMM = 0`. A block with matches must have at least 5 final literal bytes, and its last match must start at least 12 decoded bytes before the end. Literal-only blocks may be shorter. Empty output is encoded as the single byte `00`, not an empty block. All input must be consumed, and the decoded count must match exactly; aligned storage padding is not codec input.

For example, `13 61 01 00 50 74 61 69 6c 21` represents one literal `a`, seven copies from distance 1, and the five final literals `tail!`: `aaaaaaaatail!`.

## Checked decoding

```rust
use mirx::coding::Lz4;

let encoded = [0x13, b'a', 1, 0, 0x50, b't', b'a', b'i', b'l', b'!'];
let plan = Lz4::new().plan(&encoded, 13).unwrap();
let mut output = [0; 13];
assert_eq!(plan.decode_into(&mut output).unwrap(), 13);
assert_eq!(&output, b"aaaaaaaatail!");
```

`Lz4::plan` checks all sequence lengths, references, block-end restrictions and exact input/output counts without allocating history or writing output. `Lz4DecodePlan` retains immutable input and the exact output requirement. `decode_into` checks capacity before replaying that same parser; errors preserve every output byte, and success preserves the suffix. Overlapping matches use previously produced bytes directly in caller output. There is no hidden allocation or decoded staging buffer.

## Caller-workspace encoding

```rust
use mirx::coding::Lz4;

let mut table = [0; Lz4::TABLE_LEN];
let mut encoder = Lz4::new().encoder(&mut table).unwrap();
let input = [42; 128];
let mut encoded = [0; 160];
let size = encoder.encoded_len(&input).unwrap();
assert_eq!(encoder.encode_into(&input, &mut encoded).unwrap(), size);
```

`Lz4::encoder` borrows a power-of-two table of 16–65,536 `u32` entries. `Lz4::TABLE_LEN` is 1,024 entries (4 KiB), an explicit starting size rather than an allocation request. Smaller tables reduce workspace but can miss matches; larger tables do not guarantee smaller output under greedy parsing. No automatic table growth or input-size-dependent stack allocation occurs. The caller may reuse one table across independent blocks.

The encoder searches one latest candidate per endian-independent 4-byte hash, uses at most a 65,535-byte backward distance, extends matches to adjacent equal bytes, and seeds the last two match positions. Table contents reset on every pass. For the same input and table size, output is deterministic across prior workspace contents and host byte order. Table size is an encoder policy, not a parameter in the block.

`encoded_len` counts exact bytes with the same sequence emitter as `encode_into`. Encoding counts first, checks capacity, then resets the table and emits without a temporary encoded buffer. Errors preserve output; scratch table contents may change. Success preserves the output suffix. `Lz4::encoded_bound` checks the conservative `input + input / 255 + 16` bound. Input lengths above `u32::MAX` are rejected by sizing and encoding.

These APIs process contiguous byte blocks. They do not select codecs, construct IMAGE payloads, convert colors, validate media CRCs or reconstruct temporal references. Asset selection must compare full encoded and RAW storage costs, including metadata, indexes and alignment.

## Strided plane output

LZ4 `DecodeUnitRef` values use the selected tight plane rows as one logical byte stream, in original plane-index order. Geometry supplies the exact decoded length. Backward offsets count only those bytes, not row padding, allocation-only rows or inter-plane alignment gaps. Matches may cross plane boundaries and refer to bytes produced earlier in the same match; no external reference or per-plane reset is implied.

`decode_plan` validates the block and target memory requirements before execution. `UnitDecodePlan::decode_into` checks the actual destination address and capacity, then writes literals and matches directly into planned rows. Source and destination row cursors advance through physical storage without a tight staging image or expanded address index. Short repeating periods use a fixed 4-byte local value, not a dictionary allocation.

Output padding is zero. Unused low bits in packed row tails are cleared only after all matches complete: such bytes may still be history for later valid samples during decoding. Original plane indices and source regions remain available on `DecodedUnit`, which borrows only the output buffer and may outlive the encoded input. Media checksums remain a separate validation step.
