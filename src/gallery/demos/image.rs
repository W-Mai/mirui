extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Image;
use crate::components::assets::*;
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::{Children, Parent, Style};

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(30, 30, 46).into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let img_widget = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    world.insert(img_widget, Image::new(&IMG_THUMBS_UP));

    world.insert(img_widget, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(img_widget);
    }
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
