use crate::gallery::play::post::PostModel;

#[crate::component]
pub(super) struct PostSurface {
    pub(super) model: model!(PostModel),
}

#[crate::component]
pub(super) struct PostModalSurface {
    pub(super) model: model!(PostModel),
}
