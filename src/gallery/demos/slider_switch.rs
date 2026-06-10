#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Slider, Switch};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let slider = ui! {
        :(
            parent: parent
            world: world
        :)

        Slider (
            width: 200,
            height: 16,
            min: Fixed::ZERO,
            max: Fixed::from_int(100)
        ) {}
    };
    if let Some(s) = world.get_mut::<Slider>(slider) {
        s.value = Fixed::from_int(50);
    }

    ui! {
        :(
            parent: parent
            world: world
        :)

        Switch (width: 50, height: 26) {}
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
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
