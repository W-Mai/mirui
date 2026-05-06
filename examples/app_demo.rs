use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlBackend::new("mirui - app demo", 480, 320);
    let mut app = App::new(backend);

    // Build UI
    let c1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            width: Some(120),
            height: Some(80),
            ..Default::default()
        })
        .id();

    let c2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .layout(LayoutStyle {
            grow: 1.0,
            height: Some(80),
            ..Default::default()
        })
        .id();

    let c3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .layout(LayoutStyle {
            width: Some(120),
            height: Some(80),
            ..Default::default()
        })
        .id();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Some(480),
            height: Some(320),
            padding: Padding {
                top: 20,
                right: 20,
                bottom: 20,
                left: 20,
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
