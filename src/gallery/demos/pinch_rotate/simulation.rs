use alloc::vec;

use crate::anim::ease;
use crate::ecs::Entity;
use crate::input::event::sim::{SimAction, SimTimeline};
use crate::types::{DimPoint, Fixed};

pub(super) fn gesture_timeline(target: Entity) -> SimTimeline {
    let small = Fixed::from_int(40);
    let large = Fixed::from_int(80);
    let radius = Fixed::from_int(50);
    SimTimeline::new(vec![
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
    .looping(true)
}
