use super::state::PostNodes;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::post::PostModel;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

fn local_point(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(point) = local_point(world, entity, *x, *y) else {
        return false;
    };
    for (index, center_x) in [151, 265].into_iter().enumerate() {
        let dx = point.x - Fixed::from_int(center_x);
        let dy = point.y - Fixed::from_int(157);
        if dx * dx + dy * dy <= Fixed::from_int(23) * Fixed::from_int(23) {
            PostNodes::update(world, |model| model.toggle_switch(index));
            return true;
        }
    }
    false
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn post_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    PostNodes::update(world, |model| model.advance_ms(elapsed));
}

pub(super) struct PostKeyboardPlugin;

impl<B, F> Plugin<B, F> for PostKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        match ch {
            'a' | 'A' => PostNodes::update(world, |model| model.toggle_switch(0)),
            's' | 'S' => PostNodes::update(world, |model| model.toggle_switch(1)),
            ' ' => PostNodes::update(world, PostModel::toggle_running),
            _ => return false,
        }
        true
    }
}
