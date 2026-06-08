extern crate alloc;

#[cfg(feature = "std")]
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::event::sim::{SimAction, SimTimeline, sim_timeline_system};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::Style;
#[cfg(feature = "std")]
use alloc::vec;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let surface_bg = Color::rgb(13, 17, 23);
    let card_a = Color::rgb(34, 74, 44);
    let card_b = Color::rgb(82, 38, 38);
    let card_c = Color::rgb(34, 56, 86);
    let card_border = Color::rgb(48, 54, 61);
    let title_color = Color::rgb(201, 209, 217);

    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(surface_bg.into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(720),
            height: Dimension::px(360),
            padding: Padding::all(28),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            direction: FlexDirection::Column,
            grow: 1.0
        ) {
            View (
                height: 36,
                text: "MoveTo demo: simulated cursor sweeps the row, hover overlays follow",
                text_color: title_color
            ) {}
            View (height: 16) {}
            Row (
                direction: FlexDirection::Row,
                grow: 1.0
            ) {
                View (
                    bg_color: card_a,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card A",
                    text_color: title_color
                ) {}
                View (width: 16) {}
                View (
                    bg_color: card_b,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card B",
                    text_color: title_color
                ) {}
                View (width: 16) {}
                View (
                    bg_color: card_c,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card C",
                    text_color: title_color
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
    let left = Point {
        x: Fixed::from_int(80),
        y: Fixed::from_int(200),
    };
    let right = Point {
        x: Fixed::from_int(640),
        y: Fixed::from_int(200),
    };
    let timeline = SimTimeline::new(vec![
        SimAction::move_to(left, right, 2400, ease::ease_in_out_cubic),
        SimAction::wait(400),
        SimAction::move_to(right, left, 2400, ease::ease_in_out_cubic),
        SimAction::wait(400),
    ])
    .looping(true);
    app.world.insert_resource(timeline);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin);
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
