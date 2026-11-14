use crate::prelude::Fixed;

/// Where each of the butterfly's three motions is, as a turn in degrees.
///
/// `motion.rs` winds these up and `render.rs` reads them: the wings beat with
/// `flap` while the whole body drifts along a loop set by `drift_x` and
/// `drift_y`.
#[crate::component]
#[derive(Default)]
pub struct Butterfly {
    pub flap: Fixed,
    pub drift_x: Fixed,
    pub drift_y: Fixed,
}
