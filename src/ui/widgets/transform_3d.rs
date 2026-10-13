use crate::types::{Fixed, Transform3D};

/// Per-entity 3×3 homography. Absent = identity, zero cost. Composes outside the local `WidgetTransform` (2D) when both are attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform3D(pub Transform3D);

impl crate::ecs::Component for WidgetTransform3D {}

impl From<Transform3D> for WidgetTransform3D {
    fn from(t: Transform3D) -> Self {
        Self(t)
    }
}

/// Pivot for `WidgetTransform3D`, expressed as fractions of the widget rect (`Fixed::ZERO` = top / left edge, `Fixed::ONE` = bottom / right edge). Absent defaults to center `(0.5, 0.5)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformOrigin {
    pub x: Fixed,
    pub y: Fixed,
}

impl TransformOrigin {
    pub const CENTRE: Self = Self {
        x: Fixed::HALF,
        y: Fixed::HALF,
    };
    pub const TOP_LEFT: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    };

    pub fn new(x: impl Into<Fixed>, y: impl Into<Fixed>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self::CENTRE
    }
}
