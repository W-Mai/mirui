use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlBackend::new("mirui - text demo", 480, 320);
    let mut app = App::new(backend);

    let label1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .border_radius(8)
        .text("Hello, mirui!")
        .text_color(Color::rgb(255, 255, 255))
        .layout(LayoutStyle { width: Some(140), height: Some(40), ..Default::default() })
        .id();

    let label2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .border_radius(8)
        .text("ECS + Flexbox")
        .layout(LayoutStyle { width: Some(140), height: Some(40), ..Default::default() })
        .id();

    let label3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .border(Color::rgb(255, 255, 255), 2)
        .border_radius(12)
        .text("no_std :)")
        .layout(LayoutStyle { width: Some(140), height: Some(40), ..Default::default() })
        .id();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Some(480),
            height: Some(320),
            ..Default::default()
        })
        .child(label1)
        .child(label2)
        .child(label3)
        .id();

    app.set_root(root);
    app.run();
}
