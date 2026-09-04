use super::*;

#[test]
fn glyph_major_omits_records_and_resolves_extreme_ordinals_in_constant_time() {
    let map = GlyphMap::glyph_major(7, 9, 3).unwrap();
    assert_eq!((map.width(), map.height()), (7, 27));
    assert_eq!(map.cell_extent(), Some((7, 9)));
    assert_eq!(map.packing(), GlyphPacking::GlyphMajor);
    assert_eq!(map.len(), 3);
    assert_eq!(map.encoded_len(), 0);
    let mut out = [0x5a; 32];
    assert_eq!(map.encode_into(&mut out), Ok(0));
    assert_eq!(out, [0x5a; 32]);
    for index in 0..3 {
        assert_eq!(map.get(index), Region::new(0, index as u32 * 9, 7, 9).ok());
    }
    let empty = GlyphMap::glyph_major(7, 9, 0).unwrap();
    assert_eq!((empty.width(), empty.height()), (7, 0));
    assert!(empty.is_empty());
    assert_eq!(empty.get(0), None);
    for size in [(0, 1), (1, 0)] {
        assert!(matches!(
            GlyphMap::glyph_major(size.0, size.1, 0),
            Err(GlyphMapError::Grid(TileGridError::EmptyTile))
        ));
    }
    assert!(matches!(
        GlyphMap::glyph_major(1, 2, u32::MAX as usize),
        Err(GlyphMapError::SizeOverflow)
    ));
    let huge = GlyphMap::glyph_major(1, 1, u32::MAX as usize).unwrap();
    assert_eq!(huge.encoded_len(), 0);
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
fn atlas_native_and_unaligned_wire_share_bounds_empty_glyphs_and_canonical_bytes() {
    let regions = [
        Region::new(1, 1, 3, 4).unwrap(),
        Region::new(1, 1, 3, 4).unwrap(),
        Region::new(2, 2, 2, 3).unwrap(),
        Region::new(7, 9, 0, 0).unwrap(),
        Region::new(7, 0, 0, 9).unwrap(),
        Region::new(0, 9, 7, 0).unwrap(),
    ];
    let map = GlyphMap::atlas(7, 9, &regions).unwrap();
    assert_eq!(map.cell_extent(), None);
    assert_eq!(map.packing(), GlyphPacking::Atlas2D);
    assert_eq!(map.encoded_len(), GLYPH_REGION_LEN * regions.len());
    #[repr(align(4))]
    struct Bytes([u8; 2 + 6 * GLYPH_REGION_LEN]);
    let mut bytes = Bytes([0x5a; 2 + 6 * GLYPH_REGION_LEN]);
    map.encode_into(&mut bytes.0[1..]).unwrap();
    assert_eq!(bytes.0[0], 0x5a);
    assert_eq!(bytes.0[bytes.0.len() - 1], 0x5a);
    assert_eq!(
        &bytes.0[1..17],
        &[1, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0]
    );
    let wire = GlyphMap::from_records(7, 9, &bytes.0[1..bytes.0.len() - 1]).unwrap();
    for checked in [map, wire] {
        assert_eq!((checked.width(), checked.height()), (7, 9));
        assert_eq!(checked.iter().count(), regions.len());
        assert_eq!(checked.iter().last(), regions.last().copied());
        for (index, region) in regions.into_iter().enumerate() {
            assert_eq!(checked.get(index), Some(region));
        }
        assert_eq!(checked.get(regions.len()), None);
        assert_eq!(checked.get(usize::MAX), None);
        let mut iter = checked.into_iter();
        assert_eq!(iter.next(), Some(regions[0]));
        assert_eq!(iter.next_back(), Some(regions[5]));
        assert_eq!(iter.nth(1), Some(regions[2]));
        assert_eq!(iter.nth_back(1), Some(regions[3]));
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
        let mut out = [0x5a; 6 * GLYPH_REGION_LEN + 1];
        assert_eq!(
            checked.encode_into(&mut out),
            Ok(GLYPH_REGION_LEN * regions.len())
        );
        assert_eq!(&out[..], &bytes.0[1..]);
        let mut short = [0x5a; 6 * GLYPH_REGION_LEN - 1];
        assert!(matches!(
            checked.encode_into(&mut short),
            Err(GlyphMapError::BufferTooSmall { .. })
        ));
        assert_eq!(short, [0x5a; 6 * GLYPH_REGION_LEN - 1]);
    }
    let empty = GlyphMap::from_records(0, 0, &[]).unwrap();
    assert_eq!(empty.packing(), GlyphPacking::Atlas2D);
    assert!(empty.is_empty());
}

#[test]
fn malformed_or_overflowing_region_records_never_escape_validation() {
    for size in 1..GLYPH_REGION_LEN {
        assert!(matches!(
            GlyphMap::from_records(10, 10, &[0; GLYPH_REGION_LEN][..size]),
            Err(GlyphMapError::PartialRecord { .. })
        ));
    }
    for region in [
        Region::new(9, 0, 2, 1).unwrap(),
        Region::new(0, 9, 1, 2).unwrap(),
        Region::new(11, 10, 0, 0).unwrap(),
    ] {
        let source = [region];
        assert!(matches!(
            GlyphMap::atlas(10, 10, &source),
            Err(GlyphMapError::InvalidRegion {
                index: 0,
                error: RegionError::OutOfBounds
            })
        ));
        let valid = GlyphMap::atlas(20, 20, &source).unwrap();
        let mut bytes = [0; GLYPH_REGION_LEN];
        valid.encode_into(&mut bytes).unwrap();
        assert!(matches!(
            GlyphMap::from_records(10, 10, &bytes),
            Err(GlyphMapError::InvalidRegion {
                index: 0,
                error: RegionError::OutOfBounds
            })
        ));
    }
    for axis in [0, 4] {
        let mut bytes = [0; GLYPH_REGION_LEN];
        bytes[axis..axis + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[axis + 8..axis + 12].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            GlyphMap::from_records(u32::MAX, u32::MAX, &bytes),
            Err(GlyphMapError::InvalidRegion {
                index: 0,
                error: RegionError::CoordinateOverflow
            })
        ));
    }
}
