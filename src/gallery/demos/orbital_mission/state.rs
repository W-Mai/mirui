use crate::gallery::play::orbit::OrbitModel;

#[crate::component]
pub(super) struct OrbitSurface {
    pub(super) model: model!(OrbitModel),
}

#[crate::component]
pub(super) struct OrbitModalSurface {
    pub(super) model: model!(OrbitModel),
}
