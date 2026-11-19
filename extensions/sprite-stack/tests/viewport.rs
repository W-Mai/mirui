#![cfg(feature = "widget")]
use mirui::prelude::*;
use mirui_sprite_stack::{
    Camera, HeightField, Pose, SpriteStackPlugin, StackScene, StackViewport, TerrainCell,
    TerrainGrid,
};
const RED: Color = Color::rgb(200, 20, 30);
const GREEN: Color = Color::rgb(40, 160, 80);
const COLORS: [Color; 2] = [RED, GREEN];

#[mirui::compose]
fn views(source: model!(StackScene), camera: Camera) -> Entity {
    ui! {
        Row(width:32,height:16) {
            StackViewport(width:16,height:16,scene:source.clone(),camera:camera)
            StackViewport(width:16,height:16,scene:source,camera:Camera{center:[Fixed::from_int(1),Fixed::ZERO,Fixed::ZERO],..camera})
        }
    }
}
#[test]
fn two_cameras_repaint_a_live_field_and_release_retired_sources() {
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let grid = TerrainGrid::finite([1, 1], Fixed::from_int(2)).unwrap();
    let field = app
        .add_model(
            HeightField::generate(grid, &COLORS, |_| TerrainCell {
                level: 0,
                material: 0,
            })
            .unwrap(),
        )
        .finish();
    let mut scene = StackScene::bounded(1);
    let item = scene.place(field.clone(), Pose::at(0, 0, 0)).unwrap();
    let scene = app.add_model(scene).finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            scene.clone(),
            Camera {
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            },
        ),
    );
    let initial = app.snapshot().unwrap().unwrap();
    let initial = initial.read_pixels().unwrap();
    assert_eq!(initial.get_pixel(8, 6), RED);
    assert_eq!(initial.get_pixel(22, 6), RED);
    assert_ne!(initial.get_pixel(7, 6), RED);
    assert_ne!(initial.get_pixel(21, 6), RED);
    field
        .set_cell(
            [0, 0],
            TerrainCell {
                level: 0,
                material: 1,
            },
        )
        .unwrap();
    assert!(app.dirty_region().unwrap().is_some());
    let frame = app.snapshot().unwrap().unwrap();
    let pixels = frame.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(8, 6), GREEN);
    assert_eq!(pixels.get_pixel(22, 6), GREEN);
    scene.set_pose(item, Pose::at(2, 0, 0)).unwrap();
    let frame = app.snapshot().unwrap().unwrap();
    assert_eq!(frame.read_pixels().unwrap().get_pixel(12, 6), GREEN);
    scene.remove(item).unwrap();
    app.snapshot().unwrap();
    app.render_dirty().unwrap();
    field
        .set_cell(
            [0, 0],
            TerrainCell {
                level: 0,
                material: 0,
            },
        )
        .unwrap();
    assert!(app.dirty_region().unwrap().is_none());
}

#[test]
fn translated_finite_source_uses_the_same_local_cell_for_height_and_drawing() {
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let grid = TerrainGrid::finite([1, 1], Fixed::from_int(2)).unwrap();
    let field = HeightField::generate(grid, &COLORS, |_| TerrainCell {
        level: 0,
        material: 0,
    })
    .unwrap()
    .with_coordinate_map(|[x, z]| [x + Fixed::from_int(2), z]);
    let mut scene = StackScene::bounded(1);
    let ground = scene.place(field, Pose::default()).unwrap();
    assert_eq!(
        scene.height_at(ground, -Fixed::ONE, Fixed::ONE),
        Ok(Some(Fixed::ZERO))
    );
    assert_eq!(scene.height_at(ground, Fixed::ONE, Fixed::ONE), Ok(None));
    let scene = app.add_model(scene).finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            scene,
            Camera {
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            },
        ),
    );
    let frame = app.snapshot().unwrap().unwrap();
    let pixels = frame.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(6, 6), RED);
    assert_ne!(pixels.get_pixel(9, 6), RED);
}
