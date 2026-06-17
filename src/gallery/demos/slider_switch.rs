#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::widgets::{Slider, Switch};

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
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
        )
    };
    if let Some(s) = world.get_mut::<Slider>(slider) {
        s.value = Fixed::from_int(50);
    }
    //~focus-end

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Switch (width: 50, height: 26)
    };
    //~focus-end
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
