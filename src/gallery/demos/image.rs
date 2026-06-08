extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Image;
use crate::components::assets::*;
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::{Children, Parent};

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let img_widget = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    world.insert(img_widget, Image::new(&IMG_THUMBS_UP));

    world.insert(img_widget, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(img_widget);
    }

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
