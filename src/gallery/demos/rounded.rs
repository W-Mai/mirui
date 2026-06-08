use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
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

    let root = WidgetBuilder::new(world)
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
        .id();

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
    use crate::widget::Children;
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
