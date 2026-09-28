use crate::gallery::play::moss::MossModel;

#[crate::component(bind(model))]
pub(super) struct MossSurface {
    pub(super) model: MossModel,
}

#[crate::component(bind(model))]
pub(super) struct MossModalSurface {
    pub(super) model: MossModel,
}
