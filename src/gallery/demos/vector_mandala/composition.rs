use super::state::VectorMandala;
use crate::prelude::*;

#[compose]
pub fn build_widgets() {
    let now_ms = res!(optional MonoClock).with(|clock| clock.map_or(0, |clock| clock.now_ms()));

    ui! {
        VectorMandala (
            start_ms: now_ms,
            petals: 10,
            grow: 1.0
        )
    };
}
