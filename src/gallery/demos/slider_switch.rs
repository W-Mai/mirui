use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Slider, Switch};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(200),
            padding: Padding {
                top: Dimension::px(30),
                left: Dimension::px(20),
                right: Dimension::px(20),
                bottom: Dimension::px(30),
            },
            ..Default::default()
        })
        .id();

    let slider = ui! {
        :(
            parent: parent
            world: world
        :)

        View (width: 200, height: 16) [
            Slider::new(Fixed::ZERO, Fixed::from_int(100)),
        ] {}
    };
    if let Some(s) = world.get_mut::<Slider>(slider) {
        s.value = Fixed::from_int(50);
    }

    ui! {
        :(
            parent: root
            world: world
        :)

        View (width: 50, height: 26) [
            Switch::new(),
        ] {}
    };

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
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
