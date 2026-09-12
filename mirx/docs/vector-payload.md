# VECTOR payload

A VECTOR payload stores checked drawing geometry in display order. The payload contains one fixed header followed by a byte-packed operation stream.

```text
VECTOR payload
├─ VectorHeader · 8 B
│  ├─ magic               u8       0x03
│  ├─ version             u8       1
│  ├─ coordinate scale    u8       8 · Q24.8
│  ├─ flags               u8       0
│  └─ body CRC            u32 LE
└─ operation body
   ├─ operation
   ├─ operation
   ├─ ...
   └─ EOF                 u8       0x00
```

All integers are little-endian. Operations are byte-packed and may begin at any address; readers assemble scalar values from bytes instead of performing aligned loads. Container-level chunk alignment controls the physical payload address when a target requires a stronger flash or DMA contract.

## Text geometry

Linear and posed text use separate operation tags and fixed record layouts. A run selects one layout for every glyph, so no per-glyph kind byte is stored.

```text
LINEAR_GLYPH_RUN · tag 0x0d
├─ fields                 u8
├─ font                   ResourceRef
├─ ppem                   u16 LE
├─ run origin             Point<Q24.8>       8 B
├─ color                  RGBA               4 B
├─ opacity                u8
├─ glyph count            varuint
├─ LinearGlyph[count]                        18 B each
│  ├─ glyph ID            u16 LE
│  ├─ origin              Point<Q24.8>       8 B
│  └─ shaping offset      Point<Q24.8>       8 B
└─ affine transform?      6 × Q24.8          24 B
```

```text
POSED_GLYPH_RUN · tag 0x0e
├─ fields                 u8
├─ font                   ResourceRef
├─ ppem                   u16 LE
├─ run origin             Point<Q24.8>       8 B
├─ color                  RGBA               4 B
├─ opacity                u8
├─ glyph count            varuint
├─ PosedGlyph[count]                         18 B each
│  ├─ glyph ID            u16 LE
│  ├─ final origin        Point<Q24.8>       8 B
│  └─ unit tangent        Point<Q24.8>       8 B
└─ affine transform?      6 × Q24.8          24 B
```

The posed record replaces the linear shaping offset with a normalized tangent; it does not add bytes to an individual glyph record. The normal is derived as `(-tangent.y, tangent.x)`. Preflight rejects zero and non-unit tangents before allocating the decoded operation table.

The operation-level affine transform is omitted when it is identity. Posed origins already contain path placement, including shaping offsets mapped onto tangent and normal axes. The source text and semantic path are not duplicated in final render geometry.

## Shared projective scope

A group holds projective state once for all nested operations.

```text
GROUP_BEGIN · tag 0x01
├─ present slots          varuint
├─ matching GROUP_END     u32 LE
├─ affine transform?      6 × Q24.8          24 B
├─ projective transform?  9 × Q48.16         72 B
├─ opacity?               u8
├─ clip?                  ResourceRef
├─ mask?                  ResourceRef
├─ filter?                ResourceRef
├─ child operation
├─ ...
└─ GROUP_END              u8                  0x02
```

The projective matrix is emitted only when slot bit 6 is present and the matrix is non-identity. Nested scopes compose in scene order. A run keeps its optional local affine transform; the group homography is applied after that local transform.

## Ownership boundary

```text
editable content
└─ text + paragraph style + font + semantic Path
   └─ shaping and path placement
      └─ final VECTOR geometry
         ├─ linear glyph records, or
         └─ posed glyph records
```

Final VECTOR geometry is sufficient for deterministic rendering. Editable content retains the semantic text and `Path` as its authority and regenerates VECTOR geometry when either revision changes. A render-only asset stores only the final run.
