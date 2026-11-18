#![cfg(feature = "widget")]

use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui::ui::OffscreenRender;
use mirui_sprite_stack::{SpriteStack, SpriteStackAsset, format::StackDescriptor, view};
use mirx::Document;
use mirx::document::EncodeOptions;
use mirx::image::{
    AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor,
};

#[path = "support/allocations.rs"]
mod allocations;

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

#[mirui::compose]
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
    assert!(sprite.layer_spacing == Fixed::from_int(2).into());
    pitch.set(Fixed::from_int(45));
    app.render_dirty().unwrap();
    yaw.set(Fixed::from_int(30));
    app.render_dirty().unwrap();
    let hot_allocations = allocations::tracked_allocations(|| {
        yaw.set(Fixed::from_int(31));
        app.render_dirty().unwrap();
    });
    assert_eq!(hot_allocations, 0);
    scale.set(-Fixed::ONE);
    assert_eq!(
        app.render_dirty(),
        Err(mirui::render::RenderError::InvalidGeometry)
    );
    scale.set(Fixed::ONE);
    app.render_dirty().unwrap();
    app.world
        .widget_mut(first)
        .unwrap()
        .set::<mirui_sprite_stack::prop::SpriteYaw>(Fixed::from_int(180));
    assert_eq!(
        app.world.get::<SpriteStack>(first).unwrap().yaw,
        Fixed::from_int(180)
    );
}

#[mirui::compose]
fn conditional(shown: Signal<bool>, angle: prop!(Fixed)) -> Entity {
    ui! {
        Column () {
            if ${ shown.get() } {
                SpriteStack (yaw: ${ angle })
            }
        }
    }
}

#[test]
fn hidden_sprite_resumes_its_display_property_at_the_latest_angle() {
    use mirui::core::reactive::flush_signal_dirty;
    use std::{cell::Cell, rc::Rc};

    let mut app = App::headless(16, 16);
    app.with_widget(view());
    let root = app.spawn_root().id();
    let shown = Signal::new(true);
    let angle = Signal::new(Fixed::from_int(10));
    let reads = Rc::new(Cell::new(0));
    let getter = {
        let reads = Rc::clone(&reads);
        let angle = angle.clone();
        Prop::new(move || {
            reads.set(reads.get() + 1);
            angle.get()
        })
    };
    app.compose(root, conditional(shown.clone(), getter));
    let (sprite, _) = app.world.query::<SpriteStack>().iter().next().unwrap();
    shown.set(false);
    flush_signal_dirty(&mut app.world);
    let before = reads.get();
    angle.set(Fixed::from_int(45));
    flush_signal_dirty(&mut app.world);
    assert_eq!(reads.get(), before);
    assert_eq!(
        app.world.get::<SpriteStack>(sprite).unwrap().yaw,
        Fixed::from_int(10)
    );
    shown.set(true);
    flush_signal_dirty(&mut app.world);
    assert!(reads.get() > before);
    assert_eq!(
        app.world.get::<SpriteStack>(sprite).unwrap().yaw,
        Fixed::from_int(45)
    );
}
