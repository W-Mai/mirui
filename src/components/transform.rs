use crate::types::Transform;

/// Per-entity 2D affine. Absent = identity, zero cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform(pub Transform);

impl From<Transform> for WidgetTransform {
    fn from(t: Transform) -> Self {
        Self(t)
    }
}
