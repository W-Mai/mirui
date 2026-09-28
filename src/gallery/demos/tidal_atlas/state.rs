use crate::gallery::play::tidal::TideModel;

#[crate::component(bind(model))]
pub(super) struct TideSurface {
    pub(super) model: TideModel,
}

#[crate::component(bind(model))]
pub(super) struct TideModalSurface {
    pub(super) model: TideModel,
}
