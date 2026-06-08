extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{TabBar, TabContent};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let tabs = ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: Color::rgb(40, 40, 56),
            width: 480,
            height: 40
        ) [
            TabBar::new(3).with_indicator_height(3),
        ] {
            View (
                text: "Home",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            View (
                text: "Search",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            View (
                text: "Profile",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
        }
    };

    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: Color::rgb(20, 20, 30),
            width: 480,
            height: 280
        ) {
            View (
                bg_color: Color::rgb(63, 185, 80),
                text: "Home page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 0,
                },
            ] {}
            View (
                bg_color: Color::rgb(255, 165, 80),
                text: "Search page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 1,
                },
            ] {}
            View (
                bg_color: Color::rgb(210, 168, 255),
                text: "Profile page",
                text_color: Color::rgb(40, 40, 56),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 2,
                },
            ] {}
        }
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
