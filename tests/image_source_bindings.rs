use mirui::core::cache::MaxSize;
use mirui::core::reactive::{Signal, flush_signal_dirty};
use mirui::core::resource::ResourceManager;
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::surface::FramebufferAccess;
use mirui::ui::property::prop;
use mirui::ui::widgets::{Image, image::ImageSource};
use std::rc::Rc;

static RED: [u8; 4] = [255, 0, 0, 255];
static BLUE: [u8; 4] = [0, 0, 255, 255];

fn texture(bytes: &'static [u8]) -> Texture<'static> {
    Texture::from_static(bytes, 1, 1, ColorFormat::RGBA8888).unwrap()
}

#[compose]
fn picture(source: prop!(ImageSource)) -> Entity {
    ui! {
        Image (src: ${ source }, width: 8, height: 8)
    }
}

#[compose]
fn forwarded_picture(source: prop!(ImageSource)) -> Entity {
    ui! {
        picture (source)
    }
}

#[test]
fn dynamic_sources_reuse_the_image_and_follow_the_new_resource() {
    let manager = ResourceManager::new(MaxSize::Bytes(64), texture(&RED))
        .with_static("red", texture(&RED))
        .with_static("blue", texture(&BLUE));
    let mut app = App::headless(8, 8);
    app.with_default_widgets();
    app.add_resource(manager);
    let root = app.spawn_root().id();
    let source = Rc::new(Signal::new("red"));
    let read = source.clone();
    let image = app.compose(root, |cx| {
        ui! {
            forwarded_picture (${ read.get() })
        }
    });
    app.render_dirty().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &RED
    );

    source.set("blue");
    app.render_dirty().unwrap();
    assert_eq!(app.world.query::<Image>().collect(), [image]);
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &BLUE
    );
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .add_static("blue", texture(&RED));
    app.render_dirty().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &RED
    );

    app.world
        .widget_mut(image)
        .unwrap()
        .set::<prop::ImageSource>(ImageSource::default());
    app.render_dirty().unwrap();
}

#[test]
fn builder_and_dsl_sources_resume_after_hidden_updates() {
    let mut app = App::headless(16, 8);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let source = Rc::new(Signal::new(ImageSource::from("red")));
    let builder_read = source.clone();
    let builder = app.compose(root, |tree| {
        tree.spawn(
            Image::build("unused").bind_property::<prop::ImageSource>(move || builder_read.get()),
        )
    });
    let shown = Signal::new(true);
    let show = shown.clone();
    let read = source.clone();
    app.compose(root, |cx| {
        ui! {
            View () {
                if ${ show.get() } {
                    forwarded_picture (${ read.get() })
                }
            }
        }
    });
    let dsl = app
        .world
        .query::<Image>()
        .collect()
        .into_iter()
        .find(|entity| *entity != builder)
        .unwrap();
    shown.set(false);
    flush_signal_dirty(&mut app.world);
    source.set("blue".into());
    flush_signal_dirty(&mut app.world);
    assert!(
        matches!(&app.world.get::<Image>(dsl).unwrap().src, ImageSource::Texture(token) if token == "red")
    );
    shown.set(true);
    flush_signal_dirty(&mut app.world);
    for entity in [builder, dsl] {
        assert_eq!(
            app.world.get::<Image>(entity).unwrap().src,
            ImageSource::from("blue")
        );
    }
}

#[test]
fn direct_texture_handles_cross_compose_and_update_the_existing_image() {
    let first = ImageSource::from(texture(&RED));
    let second = ImageSource::from(texture(&BLUE));
    let source = Signal::new(first);
    let read = source.clone();
    let mut app = App::headless(8, 8);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let image = app.compose(root, |cx| {
        ui! { forwarded_picture (${ read.get() }) }
    });
    app.render_dirty().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &RED
    );
    source.set(second.clone());
    app.render_dirty().unwrap();
    assert_eq!(app.world.query::<Image>().collect(), [image]);
    assert_eq!(app.world.get::<Image>(image).unwrap().src, second);
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &BLUE
    );
}
