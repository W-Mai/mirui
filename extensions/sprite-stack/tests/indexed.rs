#![cfg(feature = "widget")]

use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui_sprite_stack::{SpriteStack, SpriteStackAsset, format::StackDescriptor, view};
use mirx::document::EncodeOptions;
use mirx::image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor};
use mirx::{Document, Reader};

const JET: &[u8] = include_bytes!("../../../gallery/pixel-flight/assets/jet.mirx");

fn direct_jet() -> &'static [u8] {
    let image = Reader::open(JET)
        .unwrap()
        .primary()
        .unwrap()
        .unwrap()
        .image()
        .unwrap()
        .unwrap()
        .raw()
        .unwrap();
    let surface = image.surface();
    assert_eq!(surface.sample_layout(), SampleLayout::I8);
    let palette = image.color_table().unwrap();
    let plane = image.plane(0).unwrap();
    let mut rgba = Vec::new();
    for y in 0..surface.height() {
        for &index in &plane.row(y).unwrap().unwrap()[..surface.width() as usize] {
            let color = palette.get(usize::from(index)).unwrap();
            rgba.extend_from_slice(&[color.r, color.g, color.b, color.a]);
        }
    }
    let direct = SurfaceDescriptor::new(
        surface.width(),
        surface.height(),
        SampleLayout::RGBA8888,
        ColorDescription::SRGB,
    )
    .unwrap();
    let mut file = Document::new();
    let atlas = file
        .push_image(&RawImageAsset::new(direct, &[&rgba]))
        .unwrap();
    file.set_primary(atlas).unwrap();
    file.push_extension(StackDescriptor::from_mirx(JET).unwrap().extension())
        .unwrap();
    Box::leak(
        file.encode(&EncodeOptions::new())
            .unwrap()
            .into_boxed_slice(),
    )
}

#[mirui::compose]
fn scene(
    asset: Signal<Handle<SpriteStackAsset>>,
    yaw: Signal<Fixed>,
    pitch: Signal<Fixed>,
) -> Entity {
    ui! {
        SpriteStack(width:64, height:64, asset: ${asset.get()}, yaw:${yaw.get()}, pitch:${pitch.get()})
    }
}

#[test]
fn indexed_aircraft_matches_direct_color_for_projected_and_edge_on_layers() {
    let indexed = Handle::new(SpriteStackAsset::from_mirx(JET).unwrap());
    let direct = Handle::new(SpriteStackAsset::from_mirx(direct_jet()).unwrap());
    let asset = Signal::new(indexed.clone());
    let yaw = Signal::new(Fixed::ZERO);
    let pitch = Signal::new(Fixed::ZERO);
    let mut app = App::headless(64, 64);
    app.with_default_widgets()
        .with_widget(view())
        .with_offscreen_pool_budget(96 * 1024);
    let root = app.spawn_root().id();
    app.compose(root, scene(asset.clone(), yaw.clone(), pitch.clone()));
    for (heading, elevation) in [(0, 90), (37, 36), (90, 0), (0, -90), (48, -35)] {
        yaw.set(Fixed::from_int(heading));
        pitch.set(Fixed::from_int(elevation));
        asset.set(indexed.clone());
        let actual = app.snapshot().unwrap().unwrap();
        asset.set(direct.clone());
        let reference = app.snapshot().unwrap().unwrap();
        let actual = actual.read_pixels().unwrap();
        let reference = reference.read_pixels().unwrap();
        let bg = reference.get_pixel(0, 0);
        let mut visible = 0;
        for y in 0..64 {
            for x in 0..64 {
                let expected = reference.get_pixel(x, y);
                visible += usize::from(expected != bg);
                assert_eq!(
                    actual.get_pixel(x, y),
                    expected,
                    "pose ({heading},{elevation}) pixel ({x},{y})"
                );
            }
        }
        assert!(
            visible > 30,
            "empty pose ({heading},{elevation}): {visible}"
        );
    }
}
