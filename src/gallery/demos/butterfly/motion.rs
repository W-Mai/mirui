use super::state::Butterfly;

#[mirui_macros::animate(component = Flap)]
pub fn apply_flap(turn: motion!(), butterfly: component!(mut Butterfly)) {
    butterfly.flap = turn;
}

#[mirui_macros::animate(component = DriftX)]
pub fn apply_drift_x(turn: motion!(), butterfly: component!(mut Butterfly)) {
    butterfly.drift_x = turn;
}

#[mirui_macros::animate(component = DriftY)]
pub fn apply_drift_y(turn: motion!(), butterfly: component!(mut Butterfly)) {
    butterfly.drift_y = turn;
}
