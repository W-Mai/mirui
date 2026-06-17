#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            grow: 1.0
        ) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                border_color: Color::rgb(255, 255, 255),
                border_width: 2,
                border_radius: 12,
                width: 120,
                height: 80
            )
            View (
                bg_color: Color::rgb(63, 185, 80),
                border_color: Color::rgb(30, 30, 46),
                border_width: 3,
                border_radius: 20,
                width: 120,
                height: 80
            )
            View (
                bg_color: Color::rgb(248, 81, 73),
                border_radius: 40,
                width: 120,
                height: 80
            )
            View (
                border_color: Color::rgb(210, 168, 255),
                border_width: 3,
                border_radius: 8,
                width: 120,
                height: 80
            )
        }
    };
    //~focus-end
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
