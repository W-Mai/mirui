#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            grow: 1.0
        ) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                text_color: Color::rgb(255, 255, 255),
                border_radius: 8,
                text: "Hello, mirui!",
                width: 140,
                height: 40
            ) {}
            View (
                bg_color: Color::rgb(63, 185, 80),
                border_radius: 8,
                text: "ECS + Flexbox",
                width: 140,
                height: 40
            ) {}
            View (
                bg_color: Color::rgb(248, 81, 73),
                border_color: Color::rgb(255, 255, 255),
                border_width: 2,
                border_radius: 12,
                text: "no_std :)",
                width: 140,
                height: 40
            ) {}
        }
    };
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
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

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
