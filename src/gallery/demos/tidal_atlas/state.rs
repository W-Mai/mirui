use crate::gallery::play::tidal::TideModel;

#[crate::component]
pub(super) struct TideSurface {
    pub(super) model: model!(TideModel),
}

#[crate::component]
pub(super) struct TideModalSurface {
    pub(super) model: model!(TideModel),
}
