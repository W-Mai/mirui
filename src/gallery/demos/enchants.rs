#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::widgets::Image;

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0) {
            View (bg_color: Color::rgb(88, 166, 255), height: 40, text: "Enchants Demo")
            View (
                position: Position::Absolute,
                left: 200,
                top: 150,
                width: 16,
                height: 16,
                image: Image::new("thumbs_up")
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
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
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
