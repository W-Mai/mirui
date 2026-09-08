# Font representation records

`font::RepresentationRecord` stores 16 bytes of size semantics and shared storage references. Its in-memory `FontRepresentation` also contains sample depth and decoded selection cost; these facts are derived from the bound `SurfaceDescriptor`, not serialized again.

| Bytes | Field | Meaning |
| --- | --- | --- |
|0|class:u8|0 Coverage, 1 signed distance, 2 application-defined|
|1|reserved:u8|Zero|
|2–3|design_ppem:u16|Positive design size|
|4–5|min_ppem:u16|Zero for Coverage; interval minimum otherwise|
|6–7|max_ppem:u16|Zero for Coverage; inclusive interval maximum otherwise|
|8–9|detail:u16|Zero for Coverage; SDF spread or application kind|
|10–11|surface_index:u16|Shared surface-group ordinal|
|12–15|atlas_map_offset:u32|Offset relative to ATLAS_MAPS|

All multi-byte fields are little-endian. Coverage reconstructs its fixed range from design ppem and rejects nonzero range/detail fields. SDF requires a positive spread and a valid interval containing design ppem. Application identifiers retain all `u16` values, including 0 and 1, without colliding with standard classes. Typed parsing rejects unknown classes and nonzero reserved bytes.

Coverage accepts A1/A2/A4/A8; signed distance accepts A4/A8. Application semantics can bind any understood sample layout. Decoded cost comes from the shared surface planner's tight byte count, excluding stored compression, row/allocation padding and backend scratch. Complete face binding must resolve the surface index, check glyph-map bounds and validate native metadata with `validate_for(surface)` before emission. A record alone does not prove those references are valid.

```rust
use mirx::{FontRepresentation, font::{RepresentationRecord, REPRESENTATION_RECORD_LEN},
    image::{ColorDescription, SampleLayout, SurfaceDescriptor}};

let surface = SurfaceDescriptor::new(8, 16, SampleLayout::A4, ColorDescription::NONE).unwrap();
let metadata = FontRepresentation::signed_distance(4, 3, 24, 17, 48, 64).unwrap();
let record = RepresentationRecord::new(metadata, 2).with_atlas_map_offset(32);
record.validate_for(surface).unwrap();
let mut bytes = [0; REPRESENTATION_RECORD_LEN];
record.encode_record_into(&mut bytes).unwrap();
assert_eq!(bytes, [1, 0, 24, 0, 17, 0, 48, 0, 3, 0, 2, 0, 32, 0, 0, 0]);
let decoded = RepresentationRecord::from_record(&bytes, surface).unwrap();
assert_eq!(decoded, record);
```

Reading and writing one record allocate nothing and tolerate unaligned input addresses. Short output errors preserve all bytes; success preserves the suffix. `from_record` consumes only its 16-byte prefix. A complete representation table derives count from its exact byte length; representation-major raster offsets remain in the separate `RASTER_METRICS` section.
