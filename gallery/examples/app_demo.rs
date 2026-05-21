use mirui::app::App;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlSurface::new("mirui - app demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();

    // Build UI
    let c1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let c2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .layout(LayoutStyle {
            grow: Fixed::from_f32(1.0),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let c3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
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
            padding: Padding {
                top: 20.into(),
                right: 20.into(),
                bottom: 20.into(),
                left: 20.into(),
            },
            ..Default::default()
        })
        .child(c1)
        .child(c2)
        .child(c3)
        .id();

    app.set_root(root);
    app.run();
}
