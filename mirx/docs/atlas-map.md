# Atlas maps

`image::AtlasMap` is an allocation-free view of ordered rectangles inside one logical atlas extent. It carries no font, sprite, animation, color, coding or memory-layout semantics.

Native maps borrow `Region` values. Wire maps borrow canonical 16-byte little-endian records containing `x`, `y`, `width` and `height`. Every rectangle must fit the declared extent. Empty entries use the all-zero record; shared and overlapping nonempty rectangles are valid.

```rust
use mirx::image::{AtlasMap, Region};

let regions = [
    Region::new(1, 2, 3, 4).unwrap(),
    Region::new(0, 0, 0, 0).unwrap(),
];
let map = AtlasMap::new(8, 8, &regions)?;
assert_eq!(map.get(0), Some(regions[0]));
assert_eq!(map.iter().count(), 2);
# Ok::<(), mirx::image::AtlasMapError>(())
```

Construction validates the complete borrowed map once. Indexed access and forward or reverse iteration perform no allocation. `FontAsset` and other typed asset writers serialize the map without exposing record lengths or byte emitters.
