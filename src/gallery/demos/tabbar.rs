extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::widgets::{TabBar, TabContent};

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let tabs = ui! {
        :(
            parent: parent
            world: world
        :)

        TabBar (
            bg_color: Color::rgb(40, 40, 56),
            height: 40,
            count: 3,
            indicator_height: Fixed::from_int(3)
        ) {
            View (
                text: "Home",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            )
            View (
                text: "Search",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            )
            View (
                text: "Profile",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            )
        }
    };
    //~focus-end

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(20, 20, 30),
            grow: 1.0
        ) {
            View (
                bg_color: Color::rgb(63, 185, 80),
                text: "Home page",
                text_color: Color::rgb(255, 255, 255),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 0,
                },
            ]
            View (
                bg_color: Color::rgb(255, 165, 80),
                text: "Search page",
                text_color: Color::rgb(255, 255, 255),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 1,
                },
            ]
            View (
                bg_color: Color::rgb(210, 168, 255),
                text: "Profile page",
                text_color: Color::rgb(40, 40, 56),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 2,
                },
            ]
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
