use crate::ecs::DeltaTimeMs;
use crate::gallery::play::lumen::LumenModel;

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn lumen_tick_system(model: &LumenModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}
