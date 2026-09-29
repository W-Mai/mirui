use crate::gallery::play::expeditions::ExpeditionUiModel;
use crate::gallery::play::picture::PictureModel;

#[crate::component(bind(model))]
pub(super) struct PictureSurface {
    pub(super) model: PictureModel,
}

#[crate::component(bind(expedition))]
pub(super) struct PictureExpeditionState {
    pub(super) expedition: ExpeditionUiModel,
}
