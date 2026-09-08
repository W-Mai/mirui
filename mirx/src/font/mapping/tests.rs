use super::*;

#[test]
fn cells_omit_records_and_resolve_extreme_ordinals_in_constant_time() {
    let map = GlyphMap::cells(7, 9, 3).unwrap();
    assert_eq!((map.width(), map.height()), (7, 27));
    assert_eq!(map.cell_extent(), Some((7, 9)));
    assert_eq!(map.atlas_map(), None);
    assert_eq!(map.packing(), GlyphPacking::GlyphMajor);
    assert_eq!(map.len(), 3);
    for index in 0..3 {
        assert_eq!(map.get(index), Region::new(0, index as u32 * 9, 7, 9).ok());
    }
    let empty = GlyphMap::cells(7, 9, 0).unwrap();
    assert_eq!((empty.width(), empty.height()), (7, 0));
    assert!(empty.is_empty());
    assert_eq!(empty.get(0), None);
    for size in [(0, 1), (1, 0)] {
        assert!(matches!(
            GlyphMap::cells(size.0, size.1, 0),
            Err(GlyphMapError::Grid(TileGridError::EmptyTile))
        ));
    }
    assert!(matches!(
        GlyphMap::cells(1, 2, u32::MAX as usize),
        Err(GlyphMapError::SizeOverflow)
    ));
    let huge = GlyphMap::cells(1, 1, u32::MAX as usize).unwrap();
    assert_eq!(huge.iter().count(), u32::MAX as usize);
    assert_eq!(huge.iter().last(), Region::new(0, u32::MAX - 1, 1, 1).ok());
    assert_eq!(huge.iter().nth(u32::MAX as usize - 1), huge.iter().last());
    assert_eq!(huge.iter().nth_back(u32::MAX as usize - 1), huge.get(0));
    assert_eq!(huge.get(usize::MAX), None);
    for reverse in [false, true] {
        let mut iter = huge.iter();
        assert_eq!(
            if reverse {
                iter.nth_back(usize::MAX)
            } else {
                iter.nth(usize::MAX)
            },
            None
        );
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }
}

#[test]
fn atlas_maps_retain_shared_geometry_without_font_specific_records() {
    let regions = [
        Region::new(1, 1, 3, 4).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(6, 8, 1, 1).unwrap(),
    ];
    let atlas = AtlasMap::new(7, 9, &regions).unwrap();
    let map = GlyphMap::atlas(atlas);
    assert_eq!(map.cell_extent(), None);
    assert_eq!(map.atlas_map(), Some(atlas));
    assert_eq!(map.packing(), GlyphPacking::Atlas2D);
    assert_eq!((map.width(), map.height()), (7, 9));
    assert_eq!(map.iter().collect::<alloc::vec::Vec<_>>(), regions);
}
