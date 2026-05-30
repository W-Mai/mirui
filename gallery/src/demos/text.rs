use crate::Setup;
use mirui::ecs::Entity;
use mirui::prelude::*;

pub fn build(setup: &mut Setup<'_>) -> Entity {
    let world = &mut setup.app.world;

    let label1 = WidgetBuilder::new(world)
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

    let label2 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(63, 185, 80))
        .border_radius(8)
        .text("ECS + Flexbox")
        .layout(LayoutStyle {
            width: Dimension::px(140),
            height: Dimension::px(40),
            ..Default::default()
        })
        .id();

    let label3 = WidgetBuilder::new(world)
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

    WidgetBuilder::new(world)
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
        .id()
}
