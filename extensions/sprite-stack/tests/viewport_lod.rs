#![cfg(feature = "widget")]

use mirui::{core::cache::Handle, prelude::*};
use mirui_sprite_stack::{
    Camera, Pose, SpriteStackAsset, SpriteStackPlugin, StackScene, StackViewport,
    format::{
        self, StackDescriptor,
        lod::{self, LodColor, bake},
    },
};
use mirx::{
    Document,
    document::EncodeOptions,
    extension::Extension,
    image::{AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor},
};

#[path = "support/allocations.rs"]
mod allocations;

const RED: [u8; 4] = [173, 39, 41, 255];
const BLUE: [u8; 4] = [35, 95, 181, 255];

// A blue coarse level distinguishes LOD selection from sampling the red base.
fn asset(indexed: bool, has_levels: bool) -> Handle<SpriteStackAsset> {
    let mut doc = Document::new();
    let colors = [RED, BLUE, [0; 4], [0; 4]];
    let index = u8::from(!has_levels);
    let layout = if indexed {
        SampleLayout::I2
    } else {
        SampleLayout::RGBA8888
    };
    let surface = SurfaceDescriptor::new(3, 3, layout, ColorDescription::SRGB).unwrap();
    let packed = [index * 0b01010100; 3];
    let rgba = [colors[usize::from(index)]; 9];
    let rows = [if indexed {
        &packed[..]
    } else {
        rgba.as_flattened()
    }];
    let primary = RawImageAsset::new(surface, &rows);
    let image = doc
        .push_image(&if indexed {
            primary.with_color_table(colors.as_flattened())
        } else {
            primary
        })
        .unwrap();
    doc.set_primary(image).unwrap();
    let regions = [Region::new(0, 0, 3, 3).unwrap()];
    let stack =
        StackDescriptor::new((3, 3), [0; 2], 256, AtlasMap::new(3, 3, &regions).unwrap()).unwrap();
    doc.push_extension(stack.extension()).unwrap();
    if has_levels {
        let (samples, blue) = if indexed {
            (
                bake::Samples::Indexed(colors.as_flattened()),
                LodColor::Indexed(1),
            )
        } else {
            (bake::Samples::Rgba, LodColor::Rgba(BLUE))
        };
        let level = bake::encode(stack, samples, &[1, 2], &[], 4096, |_, _| Ok(blue))
            .unwrap()
            .unwrap();
        doc.push_extension(
            Extension::owned(lod::CHUNK_TYPE, level).with_policy(format::relocation_policy()),
        )
        .unwrap();
    }
    let bytes = Box::leak(
        doc.encode(&EncodeOptions::new())
            .unwrap()
            .into_boxed_slice(),
    );
    Handle::new(SpriteStackAsset::from_mirx(bytes).unwrap())
}

#[mirui::compose]
fn pair(actual: model!(StackScene), reference: model!(StackScene), zoom: Signal<Fixed>) -> Entity {
    let reference_zoom = zoom.clone();
    ui! {
        Row(width:32,height:16) {
            StackViewport(width:16,height:16,scene:actual,camera:${ Camera {scale:zoom.get(),..Camera::overhead()} })
            StackViewport(width:16,height:16,scene:reference,camera:${ Camera {scale:reference_zoom.get(),..Camera::overhead()} })
        }
    }
}

#[test]
fn public_camera_distance_controls_perspective_in_an_ordinary_viewport() {
    use mirui_sprite_stack::ViewportCamera;

    #[mirui::compose]
    fn screen(scene: model!(StackScene), camera: Camera) -> Entity {
        ui! { StackViewport(id: "camera", width: 32, height: 32, scene: scene, camera: camera) }
    }
    let mut app = App::headless(32, 32);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let mut scene = StackScene::bounded(1);
    let id = scene.place(asset(false, true), Pose::default()).unwrap();
    let scene = app.add_model(scene).finish();
    let camera = Camera {
        scale: Fixed::from_int(4),
        ..Camera::overhead()
    };
    let root = app.spawn_root().id();
    app.compose(root, screen(scene.clone(), camera));
    let coverage = |app: &mut App<_, _>| {
        let image = app.snapshot().unwrap().unwrap();
        let pixels = image.read_pixels().unwrap();
        (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let c = pixels.get_pixel(x, y);
                [c.r, c.g, c.b, c.a] == RED
            })
            .count()
    };
    let initial = coverage(&mut app);
    assert!(initial > 0);
    scene.set_pose(id, Pose::at(0, -8, 0)).unwrap();
    assert_eq!(coverage(&mut app), initial);
    let viewport = app.world.find_by_id("camera").unwrap();
    let distance = |value| Camera {
        focal_distance: Some(Fixed::from_int(value)),
        ..camera
    };
    app.world
        .widget_mut(viewport)
        .unwrap()
        .set::<ViewportCamera>(distance(8));
    let far = coverage(&mut app);
    assert!(far > 0 && far < initial);
    scene.set_pose(id, Pose::default()).unwrap();
    assert_eq!(coverage(&mut app), initial);
    app.world
        .widget_mut(viewport)
        .unwrap()
        .set::<ViewportCamera>(distance(0));
    assert!(matches!(
        app.snapshot(),
        Err(mirui::render::RenderError::InvalidGeometry)
    ));
}

#[test]
fn viewport_switches_between_indexed_or_rgba_levels_and_the_odd_sized_base() {
    for indexed in [false, true] {
        let mut app = App::headless(32, 16);
        app.with_default_widgets().add_plugin(SpriteStackPlugin);
        let mut actual = StackScene::bounded(1);
        let mut reference = StackScene::bounded(1);
        let pose = Pose::at(-Fixed::HALF, 0, -Fixed::HALF);
        actual.place(asset(indexed, true), pose).unwrap();
        reference.place(asset(indexed, false), pose).unwrap();
        let actual = app.add_model(actual).finish();
        let reference = app.add_model(reference).finish();
        let zoom = Signal::new(Fixed::from_int(2));
        let root = app.spawn_root().id();
        app.compose(root, pair(actual, reference, zoom.clone()));
        for scale in [Fixed::from_int(2), Fixed::HALF, Fixed::from_int(2)] {
            zoom.set(scale);
            let snapshot = app.snapshot().unwrap().unwrap();
            let view = snapshot.read_pixels().unwrap();
            let mut colored = 0;
            for y in 0..16 {
                for x in 0..16 {
                    let c = view.get_pixel(x, y);
                    let r = view.get_pixel(x + 16, y);
                    let reference_blue = r == Color::rgb(BLUE[0], BLUE[1], BLUE[2]);
                    let source_color = if scale == Fixed::HALF { BLUE } else { RED };
                    assert_eq!(
                        c == Color::rgb(source_color[0], source_color[1], source_color[2]),
                        reference_blue
                    );
                    colored += usize::from(reference_blue);
                }
            }
            assert!(colored > 0);
        }
        zoom.set(Fixed::HALF);
        app.render_dirty().unwrap();
        let hot = allocations::tracked_allocations(|| {
            zoom.set(Fixed::from_ratio(7, 16));
            app.render_dirty().unwrap();
        });
        assert_eq!(hot, 0);
    }
}

#[test]
fn directed_view_uses_projected_cell_size_for_authored_lods() {
    use mirui::types::Fixed64;
    use mirui_sprite_stack::{
        ObjectPass, ViewProjection,
        raster::{Frame, Projection, Workspace},
    };
    let mut scene = StackScene::new();
    let id = scene.place(asset(true, true), Pose::default()).unwrap();
    let mut pass = ObjectPass::default();
    let mut frame = Frame::new(16, 16).unwrap();
    let mut work = Workspace::new(16, 16).unwrap();
    for (z, expected) in [(0, RED), (32, BLUE), (0, RED)] {
        scene.set_pose(id, Pose::at(0, -z, z)).unwrap();
        let camera = ViewProjection::horizontal(
            Projection {
                perspective: Fixed64::from_ratio(1, 8),
                slope: Fixed::ONE,
                scale: Fixed::from_int(2),
                center: [Fixed::from_int(8); 2],
            },
            [Fixed::ZERO; 3],
            [Fixed::ZERO, Fixed::ONE],
            Fixed::from_int(50),
        );
        let objects = pass.prepare(&scene, camera, |_, _, _| true).unwrap();
        work.render_rows(&mut frame, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })
        .unwrap();
        let pixels = frame.texture.read_pixels().unwrap();
        let visible = pixels.bytes().chunks_exact(4).filter(|c| c[3] != 0);
        assert!(visible.clone().count() > 0, "depth {z}");
        assert!(visible.into_iter().all(|c| c == expected), "depth {z}");
    }
}

#[test]
fn sampled_layer_workspace_reuses_capacity_across_membership_changes() {
    use mirui_sprite_stack::{
        Geometry, ObjectPass, PreparedSpriteStack, SparseLevel, StackSample, ViewProjection,
        raster::{Frame, Projection, Workspace},
    };
    const CELL: StackSample = StackSample {
        x: 0,
        y: 0,
        color: 0,
    };
    const SHORT: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [1, 1, 1],
        [0; 3],
        256,
        &[RED],
        &[SparseLevel::new(1, &[0, 1], &[CELL]).unwrap()],
    )
    .unwrap();
    const TALL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [1, 1, 3],
        [0; 3],
        256,
        &[BLUE],
        &[SparseLevel::new(1, &[0, 1, 2, 3], &[CELL; 3]).unwrap()],
    )
    .unwrap();
    let mut scene = StackScene::bounded(2);
    scene
        .place(Geometry::Sampled(&SHORT), Pose::at(-3, 0, 0))
        .unwrap();
    let mut second = scene
        .place(Geometry::Sampled(&TALL), Pose::at(3, 0, 0))
        .unwrap();
    let mut pass = ObjectPass::default();
    let mut frame = Frame::new(16, 16).unwrap();
    let mut work = Workspace::new(16, 4).unwrap();
    let mut draw = |pass: &mut ObjectPass, scene: &StackScene| {
        let camera = ViewProjection::horizontal(
            Projection {
                perspective: Default::default(),
                slope: Fixed::ZERO,
                scale: Fixed::ONE,
                center: [Fixed::from_int(8); 2],
            },
            [Fixed::ZERO; 3],
            [Fixed::ZERO, Fixed::ONE],
            Fixed::from_int(32),
        );
        let objects = pass.prepare(scene, camera, |_, _, _| true).unwrap();
        work.render_rows(&mut frame, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })
        .unwrap();
        let pixels = frame.texture.read_pixels().unwrap();
        let mut counts = [0, 0];
        for color in pixels.bytes().chunks_exact(4) {
            counts[0] += usize::from(color == RED);
            counts[1] += usize::from(color == BLUE);
        }
        counts
    };
    let expected = draw(&mut pass, &scene);
    assert!(expected.into_iter().all(|count| count > 0));
    let capacity = pass.storage_bytes();
    let two_largest_footprints = size_of::<[[Fixed; 2]; 64]>() * 2;
    assert!(capacity < two_largest_footprints, "{capacity} bytes");
    for _ in 0..3 {
        scene.remove(second).unwrap();
        assert_eq!(draw(&mut pass, &scene), [expected[0], 0]);
        second = scene
            .place(Geometry::Sampled(&TALL), Pose::at(3, 0, 0))
            .unwrap();
        let allocations = allocations::tracked_allocations(|| {
            assert_eq!(draw(&mut pass, &scene), expected);
        });
        assert_eq!(allocations, 0);
        assert_eq!(pass.storage_bytes(), capacity);
    }
    pass.clear();
    assert!(pass.storage_bytes() < capacity);
}

mod stage_material {
    use super::*;
    use mirui_sprite_stack::{StackStage, StackStageCommands, StackVisual};

    const SKIN: [[u8; 4]; 4] = [[20, 140, 40, 0], [200, 130, 30, 0], [255; 4], [255; 4]];

    #[mirui::model]
    struct Car {
        asset: Handle<SpriteStackAsset>,
        #[observe]
        appearance: (&'static [[u8; 4]], Fixed),
    }

    #[mirui::model]
    impl Car {
        fn asset(&self) -> Handle<SpriteStackAsset> {
            self.asset.clone()
        }

        fn change(&mut self, palette: &'static [[u8; 4]], flash: Fixed) {
            self.appearance = (palette, flash);
        }
    }

    #[mirui::component]
    struct Actor;

    #[mirui::system]
    fn populate(stage: model!(StackStage), car: model!(Car), commands: commands!()) {
        if stage.is_empty() {
            commands.stage(stage).spawn(
                Actor,
                StackVisual::from_model(car, |car| {
                    let (palette, flash) = car.appearance();
                    StackVisual::new(car.asset())
                        .with_palette(palette)
                        .with_flash(flash)
                }),
            );
        }
    }

    #[mirui::compose]
    fn view(stage: model!(StackStage)) -> Entity {
        ui! {
            Row(width: 32, height: 16) {
                StackViewport(width: 16, height: 16, stage: stage.clone(), camera: Camera { scale: Fixed::from_int(2), ..Camera::overhead() })
                StackViewport(width: 16, height: 16, stage: stage, camera: Camera { scale: Fixed::HALF, ..Camera::overhead() })
            }
        }
    }

    #[test]
    fn both_viewports_preserve_lod_and_coverage() {
        let mut app = App::headless(32, 16);
        app.with_default_widgets().add_plugin(SpriteStackPlugin);
        let stage = app.add_model(StackStage::bounded(1)).finish();
        let car = app
            .add_model(Car {
                asset: asset(true, true),
                appearance: (&SKIN, Fixed::ZERO),
            })
            .finish();
        let root = app.spawn_root().id();
        app.compose(root, view(stage.clone()));
        app.add_system(populate::system(stage.clone(), car.clone()));
        app.tick();

        let capture = |app: &mut App<_, _>, flash| {
            let snapshot = app.snapshot().unwrap().unwrap();
            let pixels = snapshot.read_pixels().unwrap();
            core::array::from_fn::<_, 2, _>(|view| {
                let [r, g, b, _] = SKIN[view];
                let expected = Color::lerp(Color::rgb(r, g, b), Color::rgb(255, 255, 255), flash);
                let x = view as i32 * 16;
                (0..16)
                    .flat_map(|dy| (0..16).map(move |dx| (x + dx, dy)))
                    .filter(|&(x, y)| pixels.get_pixel(x, y) == expected)
                    .count()
            })
        };
        let coverage = capture(&mut app, Fixed::ZERO);
        assert!(coverage[0] > coverage[1] && coverage[1] > 0);
        car.change(&SKIN, Fixed::HALF);
        assert_eq!(capture(&mut app, Fixed::HALF), coverage);
        let revision = stage.revision();
        car.change(&[[0; 4]], Fixed::ONE);
        assert_eq!(capture(&mut app, Fixed::HALF), coverage);
        assert_eq!(stage.revision(), revision);
        car.change(&SKIN, Fixed::ZERO);
        assert_eq!(capture(&mut app, Fixed::ZERO), coverage);
    }
}
