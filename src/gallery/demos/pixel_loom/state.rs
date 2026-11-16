use crate::gallery::play::pixel::PixelModel;

#[crate::component]
pub(super) struct PixelSurface {
    pub(super) model: model!(PixelModel),
}

#[crate::component]
pub(super) struct PixelModalSurface {
    pub(super) model: model!(PixelModel),
}

/// The looping preview, a view of its own so a new preview frame repaints
/// only the preview panel.
#[crate::component]
pub(super) struct PixelPreview {
    pub(super) model: model!(PixelModel),
}
