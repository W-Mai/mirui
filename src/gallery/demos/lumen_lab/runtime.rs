use crate::ecs::DeltaTimeMs;
use crate::gallery::play::lumen::LumenModel;

#[mirui_macros::system(order = Animation)]
pub(super) fn lumen_tick_system(model: model!(LumenModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
