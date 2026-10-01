use crate::gallery::play::factory::FactoryModel;

#[crate::component]
pub(super) struct FactorySurface {
    pub(super) model: model!(FactoryModel),
}

#[crate::component]
pub(super) struct FactoryModalSurface {
    pub(super) model: model!(FactoryModel),
}
