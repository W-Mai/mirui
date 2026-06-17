#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

fn header(world: &mut World, parent: Entity) -> Entity {
    //~focus-start
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
        )
    }
    //~focus-end
}

fn button_row(world: &mut World, parent: Entity) -> Entity {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (grow: 1.0) {
            View (bg_color: Color::rgb(63, 185, 80), grow: 1.0, text: "OK", border_radius: 6)
            View (bg_color: Color::rgb(248, 81, 73), grow: 1.0, text: "Cancel", border_radius: 6)
            View (bg_color: Color::rgb(210, 168, 255), grow: 1.0, text: "Maybe", border_radius: 6)
        }
    }
    //~focus-end
}

fn footer(world: &mut World, parent: Entity) -> Entity {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(50, 50, 70),
            height: 30,
            text: "Built with mirui + xrune"
        )
    }
    //~focus-end
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    header(world, parent);
    button_row(world, parent);
    footer(world, parent);
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
