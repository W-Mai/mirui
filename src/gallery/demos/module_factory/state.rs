use crate::gallery::play::factory::FactoryModel;

#[crate::component(bind(model))]
pub(super) struct FactorySurface {
    pub(super) model: FactoryModel,
}

#[crate::component(bind(model))]
pub(super) struct FactoryModalSurface {
    pub(super) model: FactoryModel,
}
