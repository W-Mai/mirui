use crate::gallery::play::expeditions::ExpeditionUiModel;
use crate::gallery::play::picture::PictureModel;

#[crate::component]
pub(super) struct PictureSurface {
    pub(super) model: model!(PictureModel),
    pub(super) expedition: model!(ExpeditionUiModel),
}
