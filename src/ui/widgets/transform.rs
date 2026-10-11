use crate::types::Transform;

/// Per-entity 2D affine. Absent = identity, zero cost.
/// Update through `commands.set::<prop::Transform>(entity, transform)` to invalidate visual output without changing layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform(pub Transform);

impl crate::ecs::Component for WidgetTransform {}

impl From<Transform> for WidgetTransform {
    fn from(t: Transform) -> Self {
        Self(t)
    }
}
