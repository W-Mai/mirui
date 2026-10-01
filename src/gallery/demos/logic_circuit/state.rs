use crate::gallery::play::circuit::CircuitModel;

#[crate::component]
pub(super) struct CircuitSurface {
    pub(super) model: model!(CircuitModel),
}

#[crate::component]
pub(super) struct CircuitModalSurface {
    pub(super) model: model!(CircuitModel),
}
