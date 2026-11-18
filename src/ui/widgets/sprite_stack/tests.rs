use super::*;
use crate::core::reactive::Signal;
use crate::prelude::*;
use alloc::boxed::Box;
use mirx::Document;
use mirx::document::EncodeOptions;
use mirx::extension::sprite_stack::StackDescriptor;
use mirx::image::{
    AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor,
};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0; 4];

fn asset(colors: [[u8; 4]; 8]) -> Handle<SpriteStackAsset> {
    let surface =
        SurfaceDescriptor::new(4, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    let planes = [colors.as_flattened()];
    let atlas = RawImageAsset::new(surface, &planes);
    let mut file = Document::new();
    let image = file.push_image(&atlas).unwrap();
    file.set_primary(image).unwrap();
    let regions = [
        Region::new(2, 0, 2, 2).unwrap(),
        Region::new(0, 0, 2, 2).unwrap(),
    ];
    let stack = StackDescriptor::new(
        (2, 2),
        [256, 256],
        256,
        AtlasMap::new(4, 2, &regions).unwrap(),
    )
    .unwrap();
    file.push_extension(stack.extension()).unwrap();
    let bytes = Box::leak(
        file.encode(&EncodeOptions::new())
            .unwrap()
            .into_boxed_slice(),
    );
    Handle::new(SpriteStackAsset::from_mirx(bytes).unwrap())
}

#[crate::compose]
fn pair(
    first: Signal<Handle<SpriteStackAsset>>,
    second: Handle<SpriteStackAsset>,
    yaw: Signal<Fixed>,
    pitch: Signal<Fixed>,
    scale: Signal<Fixed>,
    spacing: Signal<Fixed>,
) -> Entity {
    ui! {
        Row (width: 32, height: 16) {
            SpriteStack (
                id: "first",
                width: 16,
                height: 16,
                asset: ${ first.get() },
                yaw: ${ yaw.get() },
                pitch: ${ pitch.get() },
                scale: ${ scale.get() },
                layer_spacing: ${ spacing.get() }
            )
            SpriteStack (id: "second", width: 16, height: 16, asset: second, pitch: 90, scale: 2)
        }
    }
}

#[test]
fn two_sprites_share_input_and_repaint_independent_poses() {
    let mixed = asset([CLEAR, BLUE, RED, RED, BLUE, BLUE, RED, RED]);
    let all_blue = asset([BLUE; 8]);
    let first = Signal::new(mixed.clone());
    let yaw = Signal::new(Fixed::ZERO);
    let pitch = Signal::new(Fixed::from_int(90));
    let scale = Signal::new(Fixed::from_int(2));
    let spacing = Signal::new(Fixed::ONE);
    let mut app = App::headless(32, 16);
    app.with_default_widgets()
        .with_widget(view())
        .with_offscreen_pool_budget(16 * 1024);
    let root = app.spawn_root().id();
    app.compose(
        root,
        pair(
            first.clone(),
            mixed.clone(),
            yaw.clone(),
            pitch.clone(),
            scale.clone(),
            spacing.clone(),
        ),
    );
    let snapshot = app.snapshot().unwrap().unwrap();
    let samples = snapshot.read_pixels().unwrap();
    assert_eq!(samples.get_pixel(7, 7), Color::rgb(255, 0, 0));
    assert_eq!(samples.get_pixel(9, 7), Color::rgb(0, 0, 255));
    let other = samples.get_pixel(23, 7);
    assert_eq!(other, Color::rgb(255, 0, 0));
    pitch.set(Fixed::from_int(-90));
    let snapshot = app.snapshot().unwrap().unwrap();
    assert_eq!(
        snapshot.read_pixels().unwrap().get_pixel(9, 7),
        Color::rgb(255, 0, 0)
    );
    pitch.set(Fixed::ZERO);
    yaw.set(Fixed::from_int(90));
    scale.set(Fixed::from_int(3));
    spacing.set(Fixed::from_int(2));
    first.set(all_blue.clone());
    let snapshot = app.snapshot().unwrap().unwrap();
    let samples = snapshot.read_pixels().unwrap();
    assert_eq!(samples.get_pixel(8, 8), Color::rgb(0, 0, 255));
    assert_eq!(samples.get_pixel(23, 7), other);
    let first = app.world.find_by_id("first").unwrap();
    assert!(app.world.has::<OffscreenRender>(first));
    let sprite = app.world.get::<SpriteStack>(first).unwrap();
    assert!(sprite.asset.as_ref().unwrap().ptr_eq(&all_blue));
    assert_eq!(sprite.yaw, Fixed::from_int(90));
    assert_eq!(sprite.layer_spacing.0, Some(Fixed::from_int(2)));
    pitch.set(Fixed::from_int(45));
    app.render_dirty().unwrap();
    yaw.set(Fixed::from_int(30));
    app.render_dirty().unwrap();
    let hot_allocations = crate::test_support::tracked_allocations(|| {
        yaw.set(Fixed::from_int(31));
        app.render_dirty().unwrap();
    });
    assert_eq!(hot_allocations, 0);
}
