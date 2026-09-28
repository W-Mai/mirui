use super::state::VectorMandala;
use crate::prelude::*;

#[compose]
pub fn build_widgets() {
    let now_ms = cx
        .world_mut()
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    ui! {
        VectorMandala (
            start_ms: now_ms,
            petals: 10,
            grow: 1.0
        )
    };
}
