#![cfg(feature = "widget")]
use mirui::prelude::*;
use mirui_sprite_stack::{
    Camera, HeightField, Pose, SpriteStackPlugin, StackScene, StackViewport, TerrainCell,
    TerrainGrid,
};
const RED: Color = Color::rgb(200, 20, 30);
const GREEN: Color = Color::rgb(40, 160, 80);
const COLORS: [Color; 2] = [RED, GREEN];

#[test]
fn directed_camera_publishes_once_per_change_and_an_explicit_camera_takes_over() {
    use mirui::render::RenderError;
    use mirui_sprite_stack::{SceneRenderer, StackSceneHandle, ViewportCamera, raster::Frame};
    use std::{cell::Cell, rc::Rc};

    struct Directed {
        attempts: Rc<Cell<u32>>,
        image: Frame,
    }
    impl SceneRenderer for Directed {
        fn render(
            &mut self,
            _: &StackScene,
            _: [u16; 2],
        ) -> Result<Option<&mut Frame>, RenderError> {
            let count = self.attempts.get() + 1;
            self.attempts.set(count);
            if count == 2 {
                return Err(RenderError::MissingWorkspace);
            }
            self.image.with_canvas(|canvas| canvas.bytes.fill(255));
            Ok(Some(&mut self.image))
        }
        fn frame(&self) -> Option<&Frame> {
            Some(&self.image)
        }
    }
    #[mirui::compose]
    fn directed(scene: StackSceneHandle, attempts: Rc<Cell<u32>>) -> Entity {
        let viewport = StackViewport::with_renderer(
            scene,
            Directed {
                attempts,
                image: Frame::new(4, 4).unwrap(),
            },
        );
        ui! { View(id: "camera", width: 16, height: 16) [viewport] }
    }

    let mut app = App::headless(16, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let scene = app.add_model(StackScene::new()).finish();
    let attempts = Rc::new(Cell::new(0));
    let root = app.spawn_root().id();
    app.compose(root, directed(scene.clone(), attempts.clone()));
    for _ in 0..2 {
        let frame = app.snapshot().unwrap().unwrap();
        assert_eq!(
            frame.read_pixels().unwrap().get_pixel(8, 8),
            Color::rgb(255, 255, 255)
        );
    }
    assert_eq!(attempts.get(), 1);
    scene.invalidate();
    assert!(matches!(app.snapshot(), Err(RenderError::MissingWorkspace)));
    app.snapshot().unwrap();
    assert_eq!(attempts.get(), 3);
    let entity = app.world.find_by_id("camera").unwrap();
    app.world
        .widget_mut(entity)
        .unwrap()
        .set::<ViewportCamera>(Camera::default());
    scene.invalidate();
    app.snapshot().unwrap();
    assert_eq!(attempts.get(), 3);
}

#[mirui::compose]
fn views(source: model!(StackScene), camera: Camera) -> Entity {
    ui! {
        Row(width:32,height:16) {
            StackViewport(id:"left",width:16,height:16,scene:source.clone(),camera:camera)
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
fn replacing_the_scene_invalidates_the_frame_and_releases_the_old_camera_target() {
    use mirui_sprite_stack::{ViewportCamera, ViewportScene};
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let make = |material| {
        let grid = TerrainGrid::finite([1, 1], Fixed::from_int(2)).unwrap();
        let mut scene = StackScene::bounded(1);
        let target = scene
            .place(
                HeightField::generate(grid, &COLORS, |_| TerrainCell { level: 0, material })
                    .unwrap(),
                Pose::default(),
            )
            .unwrap();
        (scene, target)
    };
    let (mut old, target) = make(0);
    old.invalidate();
    let old = app.add_model(old).finish();
    let (new, _) = make(1);
    let new = app.add_model(new).finish();
    let root = app.spawn_root().id();
    app.compose(root, views(old.clone(), Camera::overhead()));
    assert_eq!(
        app.snapshot()
            .unwrap()
            .unwrap()
            .read_pixels()
            .unwrap()
            .get_pixel(8, 7),
        RED
    );
    let left = app.world.find_by_id("left").unwrap();
    app.world
        .widget_mut(left)
        .unwrap()
        .set::<ViewportScene>(new.clone());
    assert_eq!(
        app.snapshot()
            .unwrap()
            .unwrap()
            .read_pixels()
            .unwrap()
            .get_pixel(8, 7),
        GREEN
    );
    app.world
        .widget_mut(left)
        .unwrap()
        .set::<ViewportScene>(old);
    app.world
        .widget_mut(left)
        .unwrap()
        .set::<ViewportCamera>(Camera::chase(target));
    app.snapshot().unwrap();
    app.world
        .widget_mut(left)
        .unwrap()
        .set::<ViewportScene>(new);
    assert!(matches!(
        app.snapshot(),
        Err(mirui::render::RenderError::InvalidGeometry)
    ));
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
fn sampled_models_use_all_layers_and_reuse_their_projection_across_viewport_bands() {
    use mirui_sprite_stack::{Geometry, PreparedSpriteStack, SparseLevel, StackSample};
    const OFFSETS: [usize; 74] = {
        let mut offsets = [0; 74];
        offsets[73] = 2;
        offsets
    };
    const SAMPLES: [StackSample; 2] = [
        StackSample {
            x: 0,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 0,
            y: 8,
            color: 1,
        },
    ];
    static ASSET: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [1, 9, 73],
        [0; 3],
        256,
        &[[200, 20, 30, 255], [40, 160, 80, 255]],
        &[SparseLevel::new(1, &OFFSETS, &SAMPLES).unwrap()],
    )
    .unwrap();
    #[mirui::compose]
    fn viewport(scene: model!(StackScene)) -> Entity {
        ui! { StackViewport(width:32,height:48,scene:scene,camera:Camera{scale:Fixed::from_int(2),..Camera::overhead()}) }
    }
    let mut app = App::headless(32, 48);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let mut scene = StackScene::bounded(2);
    let moving = scene
        .place(Geometry::Sampled(&ASSET), Pose::at(0, 0, 6))
        .unwrap();
    let scene = app.add_model(scene).finish();
    let root = app.spawn_root().id();
    app.compose(root, viewport(scene.clone()));
    let check = |app: &mut App<_>, x| {
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(x, 12), RED);
        assert_eq!(pixels.get_pixel(x, 28), GREEN);
    };
    check(&mut app, 16);
    scene.set_pose(moving, Pose::at(4, 0, 6)).unwrap();
    check(&mut app, 24);
    scene.remove(moving).unwrap();
    let frame = app.snapshot().unwrap().unwrap();
    assert_ne!(frame.read_pixels().unwrap().get_pixel(24, 28), GREEN);
}
