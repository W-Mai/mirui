# Font representation records

`font::RepresentationRecord` is the read-only typed projection of 20 stored bytes of size semantics and shared storage references. Its `FontRepresentation` also contains sample depth and decoded selection cost; these facts are derived from the bound surface, not serialized again. `RepresentationAsset` is the semantic authoring value; `FontAsset` assigns records and section references during checked encoding.

| Bytes | Field | Meaning |
| --- | --- | --- |
|0|class:u8|0 Coverage, 1 signed distance, 2 application-defined|
|1|reserved:u8|Zero|
|2–3|design_ppem:u16|Positive design size|
|4–5|min_ppem:u16|Zero for Coverage; interval minimum otherwise|
|6–7|max_ppem:u16|Zero for Coverage; inclusive interval maximum otherwise|
|8–9|detail:u16|Zero for Coverage; SDF spread or application kind|
|10–11|surface_index:u16|Shared surface-group ordinal|
|12–15|atlas_map_offset:u32|First region-record ordinal in ATLAS_MAPS|
|16–19|atlas_map_count:u32|Region-record count; zero omits the map|

All multi-byte fields are little-endian. Coverage reconstructs its fixed range from design ppem and rejects nonzero range/detail fields. SDF requires a positive spread and a valid interval containing design ppem. Application identifiers retain all `u16` values, including 0 and 1, without colliding with standard classes. Typed parsing rejects unknown classes and nonzero reserved bytes.

Coverage accepts A1/A2/A4/A8; signed distance accepts A4/A8. Application semantics can bind any understood sample layout. Decoded cost comes from the shared surface planner's tight byte count, excluding stored compression, row/allocation padding and backend scratch. Complete face binding resolves the surface index, checks glyph-map bounds and validates native metadata before emission. A record alone does not prove those references are valid.

```rust
use mirx::font::{FontRepresentation, RepresentationAsset};

let metadata = FontRepresentation::signed_distance(4, 3, 24, 17, 48, 64).unwrap();
let representation = RepresentationAsset::new(metadata, 2).with_atlas_map(1);
assert_eq!(representation.metadata(), metadata);
assert_eq!(representation.surface_index(), 2);
assert_eq!(representation.atlas_map_index(), Some(1));
```

An omitted map uses zero offset and zero count. A nonzero offset with zero count and any overflowing range are noncanonical. Record parsing allocates nothing and tolerates unaligned input addresses. A complete representation table derives count from its exact byte length; representation-major raster offsets remain in the separate `RASTER_METRICS` section. Record serialization and stored map ranges are private writer details.
