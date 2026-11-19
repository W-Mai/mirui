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

#[test]
fn chase_keeps_the_last_live_target_position() {
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let grid = TerrainGrid::finite([1, 1], Fixed::ONE).unwrap();
    let make = |material| {
        HeightField::generate(grid, &COLORS, |_| TerrainCell { level: 0, material }).unwrap()
    };
    let mut scene = StackScene::bounded(2);
    let follow = scene.place(make(0), Pose::at(30, 0, 0)).unwrap();
    scene.place(make(1), Pose::at(32, 0, 0)).unwrap();
    let scene = app.add_model(scene).finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            scene.clone(),
            Camera {
                target: Some(follow),
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            },
        ),
    );
    let initial = app.snapshot().unwrap().unwrap();
    assert_eq!(initial.read_pixels().unwrap().get_pixel(12, 7), GREEN);
    scene.remove(follow).unwrap();
    let removed = app.snapshot().unwrap().unwrap();
    assert_eq!(removed.read_pixels().unwrap().get_pixel(12, 7), GREEN);
    let mut next = App::headless(32, 16);
    next.with_default_widgets().add_plugin(SpriteStackPlugin);
    let scene = next.add_model(StackScene::new()).finish();
    let root = next.spawn_root().id();
    next.compose(root, views(scene, Camera::chase(follow)));
    assert!(matches!(
        next.snapshot(),
        Err(mirui::render::RenderError::InvalidGeometry)
    ));
}

#[test]
fn edge_on_camera_draws_the_nearest_slice_cells_for_atlases_and_prepared_levels() {
    use mirui::core::cache::Handle;
    use mirui_sprite_stack::{
        PreparedSpriteStack, SpriteStackAsset,
        format::{SparseLevel, StackDescriptor, StackSample},
    };
    use mirx::{
        Document,
        document::EncodeOptions,
        image::{
            AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor,
        },
    };

    const COLORS: [[u8; 4]; 2] = [[40, 160, 80, 255], [200, 20, 30, 255]];
    static MODEL: PreparedSpriteStack<'_> = PreparedSpriteStack::new(
        [1, 2, 1],
        [0; 3],
        256,
        &COLORS,
        &[SparseLevel::new(
            1,
            &[0, 2],
            &[
                StackSample {
                    x: 0,
                    y: 0,
                    color: 0,
                },
                StackSample {
                    x: 0,
                    y: 1,
                    color: 1,
                },
            ],
        )
        .unwrap()],
    )
    .unwrap();
    let surface =
        SurfaceDescriptor::new(1, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    let mut file = Document::new();
    let image = file
        .push_image(&RawImageAsset::new(surface, &[COLORS.as_flattened()]))
        .unwrap();
    file.set_primary(image).unwrap();
    let regions = [Region::new(0, 0, 1, 2).unwrap()];
    let stack =
        StackDescriptor::new((1, 2), [0; 2], 256, AtlasMap::new(1, 2, &regions).unwrap()).unwrap();
    file.push_extension(stack.extension()).unwrap();
    let bytes = Box::leak(
        file.encode(&EncodeOptions::new())
            .unwrap()
            .into_boxed_slice(),
    );
    let atlas = Handle::new(SpriteStackAsset::from_mirx(bytes).unwrap());

    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let mut scene = StackScene::bounded(2);
    scene.place(atlas, Pose::at(-1, 0, 0)).unwrap();
    scene.place(&MODEL, Pose::at(1, 0, 0)).unwrap();
    let scene = app.add_model(scene).finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            scene,
            Camera {
                pitch: Fixed::ZERO,
                scale: Fixed::from_int(4),
                ..Camera::default()
            },
        ),
    );
    let frame = app.snapshot().unwrap().unwrap();
    let pixels = frame.read_pixels().unwrap();
    for x in [4, 12] {
        assert_eq!(pixels.get_pixel(x, 6), RED);
        assert_eq!(pixels.get_pixel(x, 9), RED);
        assert_ne!(pixels.get_pixel(x, 10), RED);
    }
}
