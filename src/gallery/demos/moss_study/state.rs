use crate::gallery::play::moss::MossModel;

#[crate::component]
pub(super) struct MossSurface {
    pub(super) model: model!(MossModel),
}

#[crate::component]
pub(super) struct MossModalSurface {
    pub(super) model: model!(MossModel),
}
