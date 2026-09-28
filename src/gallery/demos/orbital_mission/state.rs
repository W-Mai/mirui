use crate::gallery::play::orbit::OrbitModel;

#[crate::component(bind(model))]
pub(super) struct OrbitSurface {
    pub(super) model: OrbitModel,
}

#[crate::component(bind(model))]
pub(super) struct OrbitModalSurface {
    pub(super) model: OrbitModel,
}
