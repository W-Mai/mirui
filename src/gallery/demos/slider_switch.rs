#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui::widgets::{Slider, Switch};

#[compose]
pub fn build_widgets() {
    //~focus-start
    let slider = ui! {
        Slider (
            width: 200,
            height: 16,
            min: Fixed::ZERO,
            max: Fixed::from_int(100)
        )
    };
    if let Some(s) = cx.world_mut().get_mut::<Slider>(slider) {
        s.value = Fixed::from_int(50);
    }
    //~focus-end

    //~focus-start
    ui! {
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
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
