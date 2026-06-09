extern crate alloc;

#[cfg(feature = "std")]
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::event::sim::{SimAction, SimTimeline, sim_timeline_system};
#[cfg(feature = "std")]
use crate::plugins::{InputFeedbackPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
#[cfg(feature = "std")]
use alloc::vec;

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            direction: FlexDirection::Column,
            grow: 1.0,
            padding: Padding::all(28)
        ) {
            View (
                height: 32,
                text: "Input feedback: cursor highlight + rotary / wheel water-drop",
                text_color: Color::rgb(201, 209, 217)
            ) {}
            View (height: 20) {}
            Row (
                direction: FlexDirection::Row,
                grow: 1.0
            ) {
                View (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(34, 74, 44),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(80, 140, 90),
                    text: "Hover A",
                    text_color: Color::rgb(220, 240, 225),
                    padding: Padding::all(20)
                ) {}
                View (width: 20) {}
                View (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(38, 58, 96),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(88, 166, 255),
                    text: "Hover B",
                    text_color: Color::rgb(220, 235, 255),
                    padding: Padding::all(20)
                ) {}
                View (width: 20) {}
                View (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(82, 38, 38),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(248, 81, 73),
                    text: "Hover C",
                    text_color: Color::rgb(255, 225, 225),
                    padding: Padding::all(20)
                ) {}
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(InputFeedbackPlugin::new())
        .add_plugin(StdInstantClockPlugin);

    let left = Point {
        x: Fixed::from_int(80),
        y: Fixed::from_int(190),
    };
    let mid = Point {
        x: Fixed::from_int(320),
        y: Fixed::from_int(190),
    };
    let right = Point {
        x: Fixed::from_int(560),
        y: Fixed::from_int(190),
    };
    app.world.insert_resource(
        SimTimeline::new(vec![
            SimAction::move_to(left, mid, 1400, ease::ease_in_out_cubic),
            SimAction::wait(300),
            SimAction::move_to(mid, right, 1400, ease::ease_in_out_cubic),
            SimAction::wait(300),
            SimAction::rotate(8, 50),
            SimAction::wait(600),
            SimAction::rotate(-8, 50),
            SimAction::wait(600),
            SimAction::move_to(right, left, 1600, ease::ease_in_out_cubic),
            SimAction::wait(500),
        ])
        .looping(true),
    );
    app.add_system(sim_timeline_system::system());

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
