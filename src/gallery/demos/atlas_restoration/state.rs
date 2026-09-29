use crate::gallery::play::expeditions::ExpeditionUiModel;
use crate::gallery::play::picture::PictureModel;

#[crate::component(bind(model, expedition))]
pub(super) struct PictureSurface {
    pub(super) model: PictureModel,
    pub(super) expedition: ExpeditionUiModel,
}
