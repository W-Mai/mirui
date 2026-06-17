extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui::widgets::{Placeholder, TextInput};

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        TextInput (
            bg_color: Color::rgb(40, 40, 56),
            border_color: Color::rgb(80, 80, 100),
            border_radius: 4,
            width: 400,
            height: 28
        ) [
            Placeholder("type something..."),
        ]
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
