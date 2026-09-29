use crate::ecs::DeltaTimeMs;
use crate::gallery::play::orbit::OrbitModel;

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn orbit_tick_system(model: &OrbitModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}
