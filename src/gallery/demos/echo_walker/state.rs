use crate::gallery::play::echo::EchoModel;

#[crate::component(bind(model))]
pub(super) struct EchoSurface {
    pub(super) model: EchoModel,
}

#[crate::component(bind(model))]
pub(super) struct EchoModalSurface {
    pub(super) model: EchoModel,
}
