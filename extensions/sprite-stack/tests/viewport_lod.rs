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
