use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

fn main() {
    let backend = SdlSurface::new("mirui - text demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();

    let label1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .border_radius(8)
        .text("Hello, mirui!")
        .text_color(Color::rgb(255, 255, 255))
        .layout(LayoutStyle {
            width: Dimension::px(140),
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let label2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .border_radius(8)
        .text("ECS + Flexbox")
        .layout(LayoutStyle {
            width: Dimension::px(140),
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let label3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .border(Color::rgb(255, 255, 255), 2)
        .border_radius(12)
        .text("no_std :)")
        .layout(LayoutStyle {
            width: Dimension::px(140),
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .child(label1)
        .child(label2)
        .child(label3)
        .id();

    app.set_root(root);
    app.run();
}
