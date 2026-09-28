use crate::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct CompactCurveNodes {
    pub(super) path: PathId,
    pub(super) text: Entity,
}

#[derive(Default)]
pub(super) struct CompactCurveMotion(pub(super) super::super::motion::BrakedPhase);
