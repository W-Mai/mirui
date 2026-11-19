#![cfg(feature = "widget")]

use mirui::{core::model::CollectionError, prelude::*};
use mirui_sprite_stack::{
    HeightField, Object, Pose, StackScene, TerrainCell, TerrainError, TerrainGrid,
};

static COLORS: [Color; 2] = [Color::rgb(100, 20, 0), Color::rgb(80, 100, 40)];

#[mirui::model]
struct Game {
    scene: model!(owned StackScene),
    player: Object,
}
#[mirui::model]
impl Game {
    fn advance(&mut self) -> Result<(), CollectionError> {
        self.scene.move_forward(self.player, Fixed::ONE)
    }
}

#[test]
fn independent_scene_identities_and_live_fields_survive_registration() {
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
    let mut scene = StackScene::bounded(1);
    let origin = Pose::at(2, 4, 0);
    let player = scene.place(field.clone(), origin).unwrap();
    let other = StackScene::bounded(1).place(field.clone(), origin).unwrap();
    assert_ne!(other, player);
    let game = app
        .add_model(Game {
            scene: scene.into(),
            player,
        })
        .finish();
    let view = game.scene();
    assert_eq!(
        view.place(field.clone(), origin),
        Err(CollectionError::Capacity)
    );
    assert_eq!(view.pose(other), None);
    assert_eq!(
        view.height_at(other, Fixed::ZERO, Fixed::ZERO),
        Err(TerrainError::MissingObject)
    );
    assert_eq!(
        view.height_at(player, Fixed::from_int(2), Fixed::ZERO),
        Ok(Some(Fixed::from_int(3)))
    );
    let rev = view.revision();
    view.set_pose(player, origin).unwrap();
    assert_eq!(view.revision(), rev);
    field
        .set_cell(
            [0, 0],
            TerrainCell {
                level: 8,
                material: 1,
            },
        )
        .unwrap();
    assert_eq!(
        view.height_at(player, Fixed::from_int(2), Fixed::ZERO),
        Ok(Some(Fixed::from_int(7)))
    );
    game.advance().unwrap();
    assert_eq!(view.revision(), rev + 1);
    view.remove(player).unwrap();
    let next = view.place(field, origin).unwrap();
    assert_ne!(next, player);
    assert_eq!(view.set_pose(player, origin), Err(CollectionError::Missing));
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
