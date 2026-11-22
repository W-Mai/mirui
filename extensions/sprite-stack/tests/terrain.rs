#![cfg(feature = "widget")]

use mirui::{core::model::ModelHandle, prelude::*};
use mirui_sprite_stack::{HeightField, TerrainCell, TerrainError, TerrainGrid};

static COLORS: [Color; 2] = [Color::rgb(100, 20, 0), Color::rgb(80, 100, 40)];

#[test]
fn repeating_field_keeps_its_mapping_after_registration() {
    let grid = TerrainGrid::repeating([2, 3], Fixed::from_int(2))
        .unwrap()
        .with_elevation(Fixed::from_int(-1), Fixed::HALF)
        .unwrap();
    let ground = HeightField::generate(grid, &COLORS, |[x, z]| TerrainCell {
        level: (x + z * 2) as u8,
        material: 0,
    })
    .unwrap();
    assert_eq!(
        ground.height_at(-Fixed::HALF, -Fixed::ONE),
        Some(Fixed::from_ratio(3, 2))
    );

    let mut app = App::headless(10, 20);
    let field = app.add_model(ground).finish();
    assert_eq!(
        field.read(|value| value.height_at(-Fixed::HALF, -Fixed::ONE)),
        Some(Fixed::from_ratio(3, 2))
    );
    field
        .set_cell(
            [1, 2],
            TerrainCell {
                level: 8,
                material: 1,
            },
        )
        .unwrap();
    assert_eq!(
        field.read(|value| value.height_at(-Fixed::HALF, -Fixed::ONE)),
        Some(Fixed::from_int(3))
    );
}

#[test]
fn borrowed_planes_keep_storage_and_refuse_edits_atomically() {
    let grid = TerrainGrid::finite([2, 1], Fixed::ONE).unwrap();
    static LEVELS: [u8; 2] = [4, 5];
    let mut f = HeightField::from_planes(grid, &LEVELS[..], vec![0, 1])
        .unwrap()
        .with_palette(&COLORS)
        .unwrap();
    assert_eq!(f.levels().as_ptr(), LEVELS.as_ptr());
    assert_eq!(f.height_at(-Fixed::HALF, Fixed::ZERO), None);
    assert_eq!(
        f.set_cell(
            [0, 0],
            TerrainCell {
                level: 8,
                material: 1
            }
        ),
        Err(TerrainError::ReadOnly)
    );
    assert_eq!(f.materials(), &[0, 1]);
}
