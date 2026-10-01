use crate::gallery::play::echo::EchoModel;

#[crate::component]
pub(super) struct EchoSurface {
    pub(super) model: model!(EchoModel),
}

#[crate::component]
pub(super) struct EchoModalSurface {
    pub(super) model: model!(EchoModel),
}
