use crate::anim::Spring;
use crate::ecs::{DeltaTimeMs, World};
use crate::prelude::Fixed;
use crate::ui;

mirui_macros::animate!(AnimateTweenY, |world, entity, value| {
    ui::set_position(world, entity, Fixed::from_int(48), value);
});

#[crate::component]
pub struct SpringBall {
    pub spring: Spring,
    pub x: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spring_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .expect("Spatial Animation requires DeltaTimeMs")
        .0;
    world.for_each_stable::<SpringBall>(|world, e| {
        let (pos, settled, target, x) = {
            let Some(sb) = world.get_mut::<SpringBall>(e) else {
                return;
            };
            sb.spring.tick(dt);
            (
                sb.spring.value(),
                sb.spring.is_settled(),
                sb.spring.target,
                sb.x,
            )
        };
        ui::set_position(world, e, x, pos);
        if settled && let Some(sb) = world.get_mut::<SpringBall>(e) {
            let new_target = if target.to_int() > 150 {
                Fixed::from_int(48)
            } else {
                Fixed::from_int(220)
            };
            sb.spring.retarget(new_target, None);
        }
    });
}
//~focus-end
