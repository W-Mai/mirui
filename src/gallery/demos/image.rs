extern crate alloc;

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

        View (
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            grow: 1.0
        ) {
            View (
                width: IMG_THUMBS_UP.width as i32,
                height: IMG_THUMBS_UP.height as i32,
                image: Image::new(&IMG_THUMBS_UP)
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
