use crate::types::Transform3D;

/// Per-entity 3×3 homography. Absent = identity, zero cost.
/// Takes priority over `WidgetTransform` (2D) when both are attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform3D(pub Transform3D);

impl From<Transform3D> for WidgetTransform3D {
    fn from(t: Transform3D) -> Self {
        Self(t)
    }
}
