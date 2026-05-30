use crate::Setup;
use mirui::ecs::Entity;
use mirui::prelude::*;

pub fn build(setup: &mut Setup<'_>) -> Entity {
    let world = &mut setup.app.world;

    let card1 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(88, 166, 255))
        .border(Color::rgb(255, 255, 255), 2)
        .border_radius(12)
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let card2 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(63, 185, 80))
        .border(Color::rgb(30, 30, 46), 3)
        .border_radius(20)
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let card3 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(248, 81, 73))
        .border_radius(40)
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let outline = WidgetBuilder::new(world)
        .border(Color::rgb(210, 168, 255), 3)
        .border_radius(8)
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
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
        .child(card1)
        .child(card2)
        .child(card3)
        .child(outline)
        .id()
}
