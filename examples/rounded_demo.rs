use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlBackend::new("mirui - rounded + border", 480, 320);
    let mut app = App::new(backend);

    // Rounded cards
    let card1 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .border(Color::rgb(255, 255, 255), 2)
        .border_radius(12)
        .layout(LayoutStyle {
            width: Some(120),
            height: Some(80),
            ..Default::default()
        })
        .id();

    let card2 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(63, 185, 80))
        .border(Color::rgb(30, 30, 46), 3)
        .border_radius(20)
        .layout(LayoutStyle {
            width: Some(120),
            height: Some(80),
            ..Default::default()
        })
        .id();

    let card3 = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .border_radius(40) // pill shape
        .layout(LayoutStyle {
            width: Some(120),
            height: Some(80),
            ..Default::default()
        })
        .id();

    // Border only (no fill)
    let outline = WidgetBuilder::new(&mut app.world)
        .border(Color::rgb(210, 168, 255), 3)
        .border_radius(8)
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
            ..Default::default()
        })
        .child(card1)
        .child(card2)
        .child(card3)
        .child(outline)
        .id();

    app.set_root(root);
    app.run();
}
