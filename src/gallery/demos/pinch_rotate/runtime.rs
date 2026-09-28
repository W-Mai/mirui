use alloc::vec;

use super::composition::build_widgets;
use crate::anim::ease;
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
use crate::types::DimPoint;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, build_widgets);

    let target = app
        .world
        .find_by_id("pinch_target")
        .expect("pinch target must be composed before its timeline");
    let small = Fixed::from_int(40);
    let large = Fixed::from_int(80);
    let radius = Fixed::from_int(50);
    let timeline = SimTimeline::new(vec![
        SimAction::pinch(
            DimPoint::CENTER,
            small,
            large,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            large,
            small,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            small,
            large,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            DimPoint::CENTER,
            radius,
            Fixed::ZERO,
            Fixed::PI / Fixed::from_int(2),
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            DimPoint::CENTER,
            radius,
            Fixed::PI / Fixed::from_int(2),
            Fixed::ZERO,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            large,
            small,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
    ])
    .looping(true);
    app.world.insert_resource(timeline);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin);
}
