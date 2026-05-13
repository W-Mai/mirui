use crate::types::{Fixed, Transform3D};

/// Per-entity 3×3 homography. Absent = identity, zero cost.
/// Takes priority over `WidgetTransform` (2D) when both are attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform3D(pub Transform3D);

impl From<Transform3D> for WidgetTransform3D {
    fn from(t: Transform3D) -> Self {
        Self(t)
    }
}

/// Pivot for 2D / 3D transforms, expressed as fractions of the widget rect
/// (`Fixed::ZERO` = top / left edge, `Fixed::ONE` = bottom / right edge).
/// Absent defaults to center `(0.5, 0.5)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformOrigin {
    pub x: Fixed,
    pub y: Fixed,
}

impl TransformOrigin {
    pub const CENTRE: Self = Self {
        x: Fixed::from_raw(128),
        y: Fixed::from_raw(128),
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
