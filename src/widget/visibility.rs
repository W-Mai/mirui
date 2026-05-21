/// Marker that hides the entity (and its descendants) from layout,
/// rendering, and hit-test. Toggle by inserting / removing.
pub struct Hidden;

/// Marker that excludes the entity from hit-test only — layout and
/// rendering still apply. Use for visual overlays (cursor, debug grids,
/// drag ghosts) that must be drawn but should never intercept input.
pub struct IgnoreHitTest;
