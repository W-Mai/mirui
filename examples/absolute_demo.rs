use alloc::vec::Vec;
use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;

extern crate alloc;

fn main() {
    let backend = SdlBackend::new("mirui - absolute position demo", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Some(480),
            height: Some(320),
            ..Default::default()
        })
        .id();

    // Flex children: header + body
    let _header = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .text("Absolute Position Demo")
        .layout(LayoutStyle {
            height: Some(40),
            ..Default::default()
        })
        .id();

    let _body = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(40, 40, 60))
        .layout(LayoutStyle {
            grow: 1.0,
            ..Default::default()
        })
        .id();

    // Absolute positioned widgets overlaid on top
    let _box1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .border_radius(8)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Some(50),
            top: Some(80),
            width: Some(60),
            height: Some(60),
            ..Default::default()
        })
        .id();

    let _box2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .border_radius(8)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Some(200),
            top: Some(120),
            width: Some(80),
            height: Some(40),
            ..Default::default()
        })
        .id();

    let _img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Some(350),
            top: Some(60),
            width: Some(IMG_THUMBS_UP_WIDTH),
            height: Some(IMG_THUMBS_UP_HEIGHT),
            ..Default::default()
        })
        .id();
    app.world.insert(
        _img,
        Image::new(
            Vec::from(IMG_THUMBS_UP),
            IMG_THUMBS_UP_WIDTH,
            IMG_THUMBS_UP_HEIGHT,
        ),
    );

    let _box3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(210, 168, 255))
        .text("floating")
        .border_radius(12)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Some(100),
            top: Some(200),
            width: Some(120),
            height: Some(50),
            ..Default::default()
        })
        .id();

    // Add all as children of root
    use mirui::widget::{Children, Parent};
    for &entity in &[_header, _body, _box1, _box2, _img, _box3] {
        app.world.insert(entity, Parent(root));
        if let Some(children) = app.world.get_mut::<Children>(root) {
            children.0.push(entity);
        }
    }

    app.set_root(root);
    app.run();
}
