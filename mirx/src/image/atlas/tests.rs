use super::*;

#[test]
fn native_and_unaligned_wire_maps_have_identical_access() {
    let regions = [
        Region::new(1, 1, 3, 4).unwrap(),
        Region::new(1, 1, 3, 4).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(6, 8, 1, 1).unwrap(),
    ];
    let native = AtlasMap::new(7, 9, &regions).unwrap();
    let mut storage = [0x5a; 2 + 4 * ATLAS_REGION_LEN];
    native.encode_into(&mut storage[1..]).unwrap();
    let wire_end = storage.len() - 1;
    let wire = AtlasMap::open(7, 9, &storage[1..wire_end]).unwrap();

    assert_eq!(storage[0], 0x5a);
    assert_eq!(storage[storage.len() - 1], 0x5a);
    for map in [native, wire] {
        assert_eq!((map.width(), map.height()), (7, 9));
        assert_eq!(map.len(), regions.len());
        assert_eq!(map.iter().collect::<alloc::vec::Vec<_>>(), regions);
        assert_eq!(map.get(regions.len()), None);
        assert_eq!(map.get(usize::MAX), None);
        let mut iter = map.iter();
        assert_eq!(iter.next(), Some(regions[0]));
        assert_eq!(iter.next_back(), Some(regions[3]));
        assert_eq!(iter.nth(0), Some(regions[1]));
        assert_eq!(iter.nth_back(0), Some(regions[2]));
        assert_eq!(iter.next(), None);
    }
}

#[test]
fn canonical_encoding_checks_capacity_before_writing() {
    let regions = [Region::new(2, 3, 4, 5).unwrap()];
    let map = AtlasMap::new(8, 8, &regions).unwrap();
    let mut output = [0x5a; ATLAS_REGION_LEN + 1];
    assert_eq!(map.encode_into(&mut output), Ok(ATLAS_REGION_LEN));
    assert_eq!(output[ATLAS_REGION_LEN], 0x5a);
    assert_eq!(
        &output[..ATLAS_REGION_LEN],
        &[2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0]
    );

    let mut short = [0x5a; ATLAS_REGION_LEN - 1];
    assert_eq!(
        map.encode_into(&mut short),
        Err(AtlasMapError::BufferTooSmall {
            needed: ATLAS_REGION_LEN,
            available: ATLAS_REGION_LEN - 1,
        })
    );
    assert_eq!(short, [0x5a; ATLAS_REGION_LEN - 1]);
}

#[test]
fn malformed_records_and_regions_are_rejected() {
    for size in 1..ATLAS_REGION_LEN {
        assert_eq!(
            AtlasMap::open(10, 10, &[0; ATLAS_REGION_LEN][..size]),
            Err(AtlasMapError::PartialRecord { byte_len: size })
        );
    }
    for region in [
        Region::new(7, 9, 0, 0).unwrap(),
        Region::new(7, 0, 0, 9).unwrap(),
        Region::new(0, 9, 7, 0).unwrap(),
    ] {
        assert_eq!(
            AtlasMap::new(10, 10, &[region]),
            Err(AtlasMapError::NonCanonicalEmpty { index: 0 })
        );
    }
    for region in [
        Region::new(9, 0, 2, 1).unwrap(),
        Region::new(0, 9, 1, 2).unwrap(),
    ] {
        assert_eq!(
            AtlasMap::new(10, 10, &[region]),
            Err(AtlasMapError::InvalidRegion {
                index: 0,
                error: RegionError::OutOfBounds,
            })
        );
    }
    for axis in [0, 4] {
        let mut bytes = [0; ATLAS_REGION_LEN];
        bytes[axis..axis + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            AtlasMap::open(u32::MAX, u32::MAX, &bytes),
            Err(AtlasMapError::InvalidRegion {
                index: 0,
                error: RegionError::CoordinateOverflow,
            })
        );
    }
}

#[test]
fn empty_map_retains_its_extent() {
    let map = AtlasMap::open(0, u32::MAX, &[]).unwrap();
    assert_eq!((map.width(), map.height()), (0, u32::MAX));
    assert!(map.is_empty());
    assert_eq!(map.encoded_len(), 0);
}
