use super::state::Butterfly;

#[mirui_macros::system(order = Animation)]
pub fn butterfly_anim_system(butterflies: query!(mut Butterfly)) {
    for _ in butterflies {}
}
