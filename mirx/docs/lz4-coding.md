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

This API decodes contiguous byte blocks. It does not select codecs, construct IMAGE payloads, convert colors, validate media CRCs or reconstruct temporal references.
