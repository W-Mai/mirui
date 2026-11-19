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
