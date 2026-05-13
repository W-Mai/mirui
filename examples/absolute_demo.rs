use mirui::app::App;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;

extern crate alloc;

fn main() {
    let backend = SdlSurface::new("mirui - absolute position demo", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    // Flex children: header + body
    let _header = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .text("Absolute Position Demo")
        .layout(LayoutStyle {
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let _body = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(40, 40, 60))
        .layout(LayoutStyle {
            grow: Fixed::from_f32(1.0),
            ..Default::default()
        })
        .id();

    // Absolute positioned widgets overlaid on top
    let _box1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .border_radius(8)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(50),
            top: Dimension::px(80),
            width: Dimension::px(60),
            height: Dimension::px(60),
            ..Default::default()
        })
        .id();

    let _box2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .border_radius(8)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(200),
            top: Dimension::px(120),
            width: Dimension::px(80),
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let _img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(350),
            top: Dimension::px(60),
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    app.world.insert(_img, Image::new(&IMG_THUMBS_UP));

    let _box3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(210, 168, 255))
        .text("floating")
        .border_radius(12)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(100),
            top: Dimension::px(200),
            width: Dimension::px(120),
            height: Dimension::px(50),
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
