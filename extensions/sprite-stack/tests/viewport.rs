#![cfg(feature = "widget")]
use mirui::prelude::*;
use mirui_sprite_stack::{
    Camera, HeightField, Pose, SpriteStackPlugin, StackStage, StackStageCommands, StackViewport,
    StackVisual, StackVisualInput, TerrainCell, TerrainGrid,
};
const RED: Color = Color::rgb(200, 20, 30);
const GREEN: Color = Color::rgb(40, 160, 80);
const COLORS: [Color; 2] = [RED, GREEN];

#[test]
fn attached_content_publishes_once_per_change_and_retries_failed_frames() {
    use mirui::render::RenderError;
    use mirui_sprite_stack::{
        ContentFrame, FrameContent, FrameSize, StackStageHandle, ViewportContent, WorldCamera,
        raster::{FrameCamera, FrameEffects, Projection, SceneCanvas, SceneLayers},
    };
    use std::{cell::Cell, rc::Rc};

    const WHITE: Color = Color::rgb(255, 255, 255);
    const BLUE: Color = Color::rgb(0, 0, 255);

    struct Background(Color);
    impl SceneLayers for Background {
        fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
            let color = self.0;
            let rgba = u32::from_le_bytes([color.r, color.g, color.b, color.a]);
            tile.paint_background(|_, _| (), |_, _| (), |_, _, _, _| rgba);
        }
    }

    struct Procedural {
        attempts: Rc<Cell<u32>>,
        color: Color,
    }
    impl FrameContent for Procedural {
        fn camera(&self, _: &(), _: FrameSize) -> Result<WorldCamera, RenderError> {
            Ok(WorldCamera {
                camera: FrameCamera {
                    projection: Projection {
                        perspective: Default::default(),
                        slope: Fixed::ZERO,
                        scale: Fixed::ONE,
                        center: [Fixed::from_int(8); 2],
                    },
                    position: [Fixed::ZERO; 3],
                    yaw: Fixed::ZERO,
                    pitch: Fixed::ZERO,
                    roll: Fixed::ZERO,
                },
                pivot: Fixed::from_int(8),
                roll: false,
            })
        }

        fn prepare(
            &mut self,
            _: &(),
            _: ContentFrame,
        ) -> Result<(impl SceneLayers, impl FrameEffects), RenderError> {
            let count = self.attempts.get() + 1;
            self.attempts.set(count);
            if count == 2 {
                return Err(RenderError::MissingWorkspace);
            }
            Ok((Background(self.color), ()))
        }
    }
    #[mirui::compose]
    fn procedural(stage: StackStageHandle, attempts: Rc<Cell<u32>>) -> Entity {
        ui! {
            StackViewport (id: "world", width: 16, height: 16, stage: stage) [
                ViewportContent::new(Procedural {
                    attempts,
                    color: WHITE,
                }),
            ]
        }
    }

    let mut app = App::headless(16, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(0)).finish();
    let attempts = Rc::new(Cell::new(0));
    let root = app.spawn_root().id();
    let view = app.compose(root, procedural(stage, attempts.clone()));
    for _ in 0..2 {
        let frame = app.snapshot().unwrap().unwrap();
        assert_eq!(
            frame.read_pixels().unwrap().get_pixel(8, 8),
            Color::rgb(255, 255, 255)
        );
    }
    assert_eq!(attempts.get(), 1);
    let replacement = app.add_model(StackStage::bounded(0)).finish();
    app.world
        .widget_mut(view)
        .unwrap()
        .set::<mirui_sprite_stack::ViewportStage>(replacement);
    assert!(matches!(app.snapshot(), Err(RenderError::MissingWorkspace)));
    app.snapshot().unwrap();
    assert_eq!(attempts.get(), 3);

    let world = app.world.find_by_id("world").unwrap();
    let replacement = Rc::new(Cell::new(0));
    app.world.insert(
        world,
        ViewportContent::new(Procedural {
            attempts: replacement.clone(),
            color: BLUE,
        }),
    );
    let frame = app.snapshot().unwrap().unwrap();
    assert_eq!(frame.read_pixels().unwrap().get_pixel(8, 8), BLUE);
    assert_eq!(replacement.get(), 1);
    app.world.remove::<ViewportContent>(world);
    let frame = app.snapshot().unwrap().unwrap();
    assert_ne!(frame.read_pixels().unwrap().get_pixel(8, 8), BLUE);
    assert_eq!(attempts.get(), 3);
    assert_eq!(replacement.get(), 1);
}

#[mirui::compose]
fn views(source: model!(StackStage), camera: Camera) -> Entity {
    ui! {
        Row(width:32,height:16) {
            StackViewport(id:"left",width:16,height:16,stage:source.clone(),camera:camera)
            StackViewport(width:16,height:16,stage:source,camera:Camera{center:[Fixed::from_int(1),Fixed::ZERO,Fixed::ZERO],..camera})
        }
    }
}
struct FixtureVisual(Option<StackVisualInput>);

#[mirui::component]
struct FixtureMember;

#[mirui::system]
fn mount_fixture(stage: model!(StackStage), input: res!(mut FixtureVisual), commands: commands!()) {
    if let Some(visual) = input.0.take() {
        commands.stage(stage).spawn(FixtureMember, visual);
    }
}

#[mirui::system]
fn detach_fixture(members: query!(Entity, FixtureMember, StackVisual), commands: commands!()) {
    for (entity, _, _) in members {
        commands.remove::<StackVisual>(entity);
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
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let pose = Signal::new(Pose::default());
    let visual = StackVisual::from_model(&field, {
        let pose = pose.clone();
        move |field| StackVisual::new(field.clone()).at(pose.get())
    });
    app.add_resource(FixtureVisual(Some(visual)));
    app.add_system(mount_fixture::system(stage.clone()));
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            stage,
            Camera {
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            },
        ),
    );
    app.tick();
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
    pose.set(Pose::at(2, 0, 0));
    let frame = app.snapshot().unwrap().unwrap();
    assert_eq!(frame.read_pixels().unwrap().get_pixel(12, 6), GREEN);
    app.add_system(detach_fixture::system());
    app.tick();
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

struct EdgeSprites(Option<[mirui_sprite_stack::Geometry; 2]>);

#[mirui::component]
struct EdgeSprite;

#[mirui::system]
fn populate_edges(
    stage: model!(StackStage),
    sprites: res!(mut EdgeSprites),
    commands: commands!(),
) {
    let Some(sprites) = sprites.0.take() else {
        return;
    };
    let mut stage = commands.stage(stage);
    for (geometry, x) in sprites.into_iter().zip([-1, 1]) {
        stage.spawn(EdgeSprite, StackVisual::new(geometry).at(Pose::at(x, 0, 0)));
    }
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

    #[mirui::compose]
    fn view(stage: model!(StackStage)) -> Entity {
        ui! {
            StackViewport(
                width: 16,
                height: 16,
                stage: stage,
                camera: Camera { pitch: Fixed::ZERO, scale: Fixed::from_int(4), ..Camera::default() }
            )
        }
    }

    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(2)).finish();
    let root = app.spawn_root().id();
    app.compose(root, view(stage.clone()));
    app.add_resource(EdgeSprites(Some([atlas.into(), (&MODEL).into()])));
    app.add_system(populate_edges::system(stage));
    app.tick();
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
    assert_eq!(field.height_at(-Fixed::ONE, Fixed::ONE), Some(Fixed::ZERO));
    assert_eq!(field.height_at(Fixed::ONE, Fixed::ONE), None);
    let stage = app.add_model(StackStage::bounded(1)).finish();
    app.add_resource(FixtureVisual(Some(StackVisual::new(field).into())));
    app.add_system(mount_fixture::system(stage.clone()));
    let root = app.spawn_root().id();
    app.compose(
        root,
        views(
            stage,
            Camera {
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            },
        ),
    );
    app.tick();
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
    fn viewport(stage: model!(StackStage)) -> Entity {
        ui! { StackViewport(width:32,height:48,stage:stage,camera:Camera{scale:Fixed::from_int(2),..Camera::overhead()}) }
    }
    let mut app = App::headless(32, 48);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let pose = Signal::new(Pose::at(0, 0, 6));
    let visual = StackVisual::from_model(&stage, {
        let pose = pose.clone();
        move |_| StackVisual::new(Geometry::Sampled(&ASSET)).at(pose.get())
    });
    app.add_resource(FixtureVisual(Some(visual)));
    app.add_system(mount_fixture::system(stage.clone()));
    let root = app.spawn_root().id();
    app.compose(root, viewport(stage));
    app.tick();
    let check = |app: &mut App<_>, x| {
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(x, 12), RED);
        assert_eq!(pixels.get_pixel(x, 28), GREEN);
    };
    check(&mut app, 16);
    pose.set(Pose::at(4, 0, 6));
    check(&mut app, 24);
    app.add_system(detach_fixture::system());
    app.tick();
    let frame = app.snapshot().unwrap().unwrap();
    assert_ne!(frame.read_pixels().unwrap().get_pixel(24, 28), GREEN);
}
