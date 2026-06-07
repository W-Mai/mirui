use crate::Setup;
use mirui::ecs::{Entity, World};
use mirui::prelude::*;

fn header(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(88, 166, 255),
            height: 40,
            text: "Hello xrune!",
            border_radius: 8
        ) {}
    }
}

fn button_row(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (direction: FlexDirection::Row, grow: 1.0) {
            View (bg_color: Color::rgb(63, 185, 80), grow: 1.0, text: "OK", border_radius: 6) {}
            View (bg_color: Color::rgb(248, 81, 73), grow: 1.0, text: "Cancel", border_radius: 6) {}
            View (bg_color: Color::rgb(210, 168, 255), grow: 1.0, text: "Maybe", border_radius: 6) {}
        }
    }
}

fn footer(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(50, 50, 70),
            height: 30,
            text: "Built with mirui + xrune"
        ) {}
    }
}

pub fn build(setup: &mut Setup<'_>) -> Entity {
    let world = &mut setup.app.world;
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();
    header(world, root);
    button_row(world, root);
    footer(world, root);
    root
}
