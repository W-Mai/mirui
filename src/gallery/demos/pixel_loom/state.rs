use crate::gallery::play::pixel::PixelModel;

#[crate::component(bind(model))]
pub(super) struct PixelSurface {
    pub(super) model: PixelModel,
}

#[crate::component(bind(model))]
pub(super) struct PixelModalSurface {
    pub(super) model: PixelModel,
}
