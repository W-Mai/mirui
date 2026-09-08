# Atlas maps

`image::AtlasMap` is an allocation-free view of ordered rectangles inside one logical atlas extent. It carries no font, sprite, animation, color, coding or memory-layout semantics.

Native maps borrow `Region` values. Wire maps borrow canonical 16-byte little-endian records containing `x`, `y`, `width` and `height`. Every rectangle must fit the declared extent. Empty entries use the all-zero record; shared and overlapping nonempty rectangles are valid.

```rust
use mirx::image::{ATLAS_REGION_LEN, AtlasMap, Region};

let regions = [
    Region::new(1, 2, 3, 4).unwrap(),
    Region::new(0, 0, 0, 0).unwrap(),
];
let native = AtlasMap::new(8, 8, &regions)?;
let mut bytes = [0; 2 * ATLAS_REGION_LEN];
native.encode_into(&mut bytes)?;

let wire = AtlasMap::open(8, 8, &bytes)?;
assert_eq!(wire.get(0), Some(regions[0]));
assert_eq!(wire.iter().count(), 2);
# Ok::<(), mirx::image::AtlasMapError>(())
```

Construction validates the complete borrowed map once. Indexed access, forward and reverse iteration, and canonical encoding perform no allocation. Encoding checks capacity before writing and preserves the output suffix.
