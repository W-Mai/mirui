#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Image;
use crate::components::assets::*;
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

        Column (
            bg_color: Color::rgb(30, 30, 46),
            width: 480,
            height: 320
        ) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                text: "Absolute Position Demo",
                height: 40
            ) {}
            View (
                bg_color: Color::rgb(40, 40, 60),
                grow: 1.0
            ) {}
            View (
                bg_color: Color::rgb(248, 81, 73),
                border_radius: 8,
                position: Position::Absolute,
                left: 50,
                top: 80,
                width: 60,
                height: 60
            ) {}
            View (
                bg_color: Color::rgb(63, 185, 80),
                border_radius: 8,
                position: Position::Absolute,
                left: 200,
                top: 120,
                width: 80,
                height: 40
            ) {}
            Image (
                position: Position::Absolute,
                left: 350,
                top: 60,
                width: IMG_THUMBS_UP.width as i32,
                height: IMG_THUMBS_UP.height as i32,
                texture: &IMG_THUMBS_UP
            ) {}
            View (
                bg_color: Color::rgb(210, 168, 255),
                text: "floating",
                border_radius: 12,
                position: Position::Absolute,
                left: 100,
                top: 200,
                width: 120,
                height: 50
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
