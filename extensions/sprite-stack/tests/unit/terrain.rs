use super::*;
use mirui::{core::model::ModelHandle, prelude::App};

#[test]
fn shared_height_bounds_follow_successful_plane_changes() {
    static COLORS: [Color; 1] = [Color::rgb(1, 2, 3)];
    let grid = TerrainGrid::repeating([2, 1], Fixed::ONE)
        .unwrap()
        .with_elevation((-1).into(), Fixed::HALF)
        .unwrap();
    assert!(matches!(
        HeightField::from_planes(grid, &[][..], &[][..]),
        Err(TerrainError::InvalidPlanes)
    ));
    let field = HeightField::from_planes(grid, alloc::vec![2, 2], alloc::vec![0, 0])
        .unwrap()
        .with_palette(&COLORS)
        .unwrap();
    let mut app = App::headless(8, 8);
    let handle = app.add_model(field).finish();
    let copy = handle.clone();
    let bounds = || copy.read(HeightField::height_bounds);
    let cell = |level| TerrainCell { level, material: 0 };
    assert_eq!(bounds(), [(-1).into(), Fixed::ZERO]);
    handle.set_cell([0, 0], cell(8)).unwrap();
    assert_eq!(bounds(), [(-1).into(), 3.into()]);
    assert_eq!(
        handle.set_cell(
            [0, 0],
            TerrainCell {
                level: 0,
                material: 1
            }
        ),
        Err(TerrainError::InvalidMaterial)
    );
    assert_eq!(
        handle.set_cell([2, 0], cell(255)),
        Err(TerrainError::OutsideGrid)
    );
    assert_eq!(bounds(), [(-1).into(), 3.into()]);
    handle.set_cell([0, 0], cell(0)).unwrap();
    assert_eq!(bounds(), [(-1).into(), Fixed::ZERO]);
    handle.set_cell([1, 0], cell(0)).unwrap();
    assert_eq!(bounds(), [(-1).into(); 2]);
    let mut flash = HeightField::from_planes(grid, &[4, 4][..], &[0, 0][..]).unwrap();
    assert_eq!(
        flash.set_cell([0, 0], cell(255)),
        Err(TerrainError::ReadOnly)
    );
    assert_eq!(flash.height_bounds(), [(-1).into(), Fixed::ONE]);
}
