use crate::gallery::play::pixel::PixelModel;

#[crate::component]
pub(super) struct PixelSurface {
    pub(super) model: model!(PixelModel),
}

#[crate::component]
pub(super) struct PixelModalSurface {
    pub(super) model: model!(PixelModel),
}
