use super::*;
use crate::{
    image::{ColorDescription, GroupPlanes, SampleLayout, SurfaceDescriptor},
    media::{CodingRecord, UnitSelectionEncoding},
};
use alloc::{vec, vec::Vec};

#[test]
fn all_selection_forms_match_spatial_intersections_and_stay_within_probe_bounds() {
    let grid = TileGrid::new(97, 51, 3, 2).unwrap();
    for step in [1, 2, 7, 257, 1000] {
        let chosen: Vec<_> = (0..grid.len() as u32).step_by(step).collect();
        for encoding in [UnitSelectionEncoding::List, UnitSelectionEncoding::Bitmap] {
            let mut bytes = vec![0; encoding.encoded_len(grid.len() as u32, &chosen).unwrap()];
            encoding
                .encode_into(grid.len() as u32, &chosen, &mut bytes)
                .unwrap();
            let sparse = match encoding {
                UnitSelectionEncoding::List => UnitSelection::list(grid.len() as u32, &bytes),
                UnitSelectionEncoding::Bitmap => UnitSelection::bitmap(grid.len() as u32, &bytes),
            }
            .unwrap();
            for selection in [sparse, UnitSelection::all(grid.len() as u32).unwrap()] {
                for (x, y, width, height) in [
                    (0, 0, 97, 51),
                    (1, 1, 4, 3),
                    (96, 50, 1, 1),
                    (9, 0, 3, 51),
                    (0, 20, 97, 1),
                    (33, 7, 0, 41),
                    (0, 51, 97, 0),
                ] {
                    let region = Region::new(x, y, width, height).unwrap();
                    let expected: Vec<_> = selection
                        .iter()
                        .filter(|cell| grid.get(*cell as usize).unwrap().intersects(region))
                        .collect();
                    let mut cursor = RegionCells::new(grid, selection, region).unwrap();
                    assert!(cursor.size_hint().1.unwrap() >= expected.len());
                    for (index, cell) in expected.iter().enumerate() {
                        assert_eq!(cursor.next(), Some(*cell));
                        assert!(cursor.size_hint().1.unwrap() >= expected.len() - index - 1);
                    }
                    assert_eq!(cursor.next(), None);
                    assert_eq!(cursor.next(), None);
                    assert_eq!(cursor.size_hint(), (0, Some(0)));
                    assert!(cursor.probes <= cursor.work_bound, "{region:?}, {step}");
                }
            }
        }
    }
}

#[test]
fn extreme_full_and_sparse_grids_jump_without_expanding_cells_or_empty_rows() {
    let grid = TileGrid::new(u32::MAX, 1, 1, 1).unwrap();
    let selection = UnitSelection::all(u32::MAX).unwrap();
    let mut cursor =
        RegionCells::new(grid, selection, Region::new(u32::MAX - 2, 0, 2, 1).unwrap()).unwrap();
    assert_eq!(cursor.work_bound, 5);
    assert_eq!(cursor.next(), Some(u32::MAX - 2));
    assert_eq!(cursor.next(), Some(u32::MAX - 1));
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.probes, 2);
    let rows = u32::MAX / 3;
    let grid = TileGrid::new(3, rows, 1, 1).unwrap();
    let bytes: Vec<_> = [0, u32::MAX - 2, u32::MAX - 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let selection = UnitSelection::list(u32::MAX, &bytes).unwrap();
    let mut cursor =
        RegionCells::new(grid, selection, Region::new(1, 0, 1, rows).unwrap()).unwrap();
    assert_eq!(cursor.work_bound, 4);
    assert_eq!(cursor.next(), Some(u32::MAX - 2));
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.probes, 1);
    let mut cursor =
        RegionCells::new(grid, selection, Region::new(1, 0, 1, rows - 1).unwrap()).unwrap();
    assert_eq!(cursor.work_bound, 0);
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.probes, 0);
}

#[test]
fn planar_units_keep_their_own_coordinates_and_exact_data_ranges() {
    let surface = SurfaceDescriptor::new(
        9,
        5,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let data: Vec<_> = (0..30).collect();
    let group = UnitGroup::builder(surface, CodingRecord::RAW, &data)
        .with_planes(GroupPlanes::Plane(1))
        .with_tiles(1, 1)
        .build()
        .unwrap();
    let query = Region::new(1, 1, 2, 2).unwrap();
    let units = group.units_in(query).unwrap();
    let bound = units.work_bound();
    for (unit, cell) in units.zip([6, 7, 11, 12]) {
        assert_eq!(unit.cell(), cell);
        assert_eq!(unit.planes(), GroupPlanes::Plane(1));
        assert_eq!(unit.data_range(), cell * 2..cell * 2 + 2);
        assert_eq!(
            unit.data(),
            &data[(cell * 2) as usize..(cell * 2 + 2) as usize]
        );
        assert!(unit.region().intersects(query));
    }
    assert_eq!(bound, 9);
    assert!(matches!(
        group.units_in(Region::new(4, 2, 2, 1).unwrap()),
        Err(RegionError::OutOfBounds)
    ));
    assert_eq!(
        group
            .units_in(Region::new(5, 3, 0, 0).unwrap())
            .unwrap()
            .count(),
        0
    );
}
