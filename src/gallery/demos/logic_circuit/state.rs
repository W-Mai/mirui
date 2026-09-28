use crate::gallery::play::circuit::CircuitModel;

#[crate::component(bind(model))]
pub(super) struct CircuitSurface {
    pub(super) model: CircuitModel,
}

#[crate::component(bind(model))]
pub(super) struct CircuitModalSurface {
    pub(super) model: CircuitModel,
}
