/// Marker that hides the entity (and its descendants) from layout,
/// rendering, and hit-test. Toggle by inserting / removing.
pub struct Hidden;

/// Marks a widget as an eligible pointer hit-test target.
///
/// Layout and clipping still traverse widgets without this marker; only the
/// deepest marked widget under the pointer becomes the event target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HitTarget;

/// Marker that excludes the entity from hit-test only — layout and
/// rendering still apply. Use for visual overlays (cursor, debug grids,
/// drag ghosts) that must be drawn but should never intercept input.
pub struct IgnoreHitTest;
