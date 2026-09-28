use crate::gallery::play::post::PostModel;

#[crate::component(bind(model))]
pub(super) struct PostSurface {
    pub(super) model: PostModel,
}

#[crate::component(bind(model))]
pub(super) struct PostModalSurface {
    pub(super) model: PostModel,
}
