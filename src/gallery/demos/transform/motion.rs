use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::WidgetTransform;

pub struct Spinner {
    pub angle: Fixed,
    pub speed: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spin_system(world: &mut World) {
    world.for_each_stable::<Spinner>(|world, e| {
        let next = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            s.angle
        } else {
            return;
        };
        world.insert(e, WidgetTransform(Transform::rotate_deg(next)));
        world.invalidate(e);
    });
}
//~focus-end
