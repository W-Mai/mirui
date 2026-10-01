use crate::ecs::DeltaTimeMs;
use crate::gallery::play::orbit::OrbitModel;

#[mirui_macros::system(order = Animation)]
pub(super) fn orbit_tick_system(model: model!(OrbitModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
