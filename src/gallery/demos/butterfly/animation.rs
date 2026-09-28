use super::state::Butterfly;
use crate::prelude::World;

#[mirui_macros::system(order = ANIMATION)]
pub fn butterfly_anim_system(world: &mut World) {
    world.for_each_stable::<Butterfly>(|world, e| {
        world.invalidate(e);
    });
}
