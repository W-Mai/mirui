extern crate alloc;

use crate::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
#[cfg(feature = "std")]
use crate::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{DeltaTimeMs, Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui;

mirui_macros::animate!(AnimateTweenY, |world, entity, value| {
    ui::set_position(world, entity, Fixed::from_int(50), value);
});

pub struct SpringBall {
    pub spring: Spring,
    pub x: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spring_system(world: &mut World) {
    let dt = world.resource::<DeltaTimeMs>().map_or(16, |r| r.0);
    let mut entities = alloc::vec::Vec::new();
    world.query::<SpringBall>().collect_into(&mut entities);

    for e in entities {
        let (pos, settled, target, x) = {
            let Some(sb) = world.get_mut::<SpringBall>(e) else {
                continue;
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
                Fixed::from_int(30)
            } else {
                Fixed::from_int(250)
            };
            sb.spring.retarget(new_target, None);
        }
    }
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 25,
            top: 5,
            width: 70,
            height: 14,
            text: "Tween"
        )
    };
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 140,
            top: 5,
            width: 70,
            height: 14,
            text: "Spring"
        )
    };
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 270,
            top: 5,
            width: 70,
            height: 14,
            text: "Elastic"
        )
    };

    //~focus-start
    let tween_ball = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(248, 81, 73),
            position: Position::Absolute,
            left: 50,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        )
    };
    world.insert(
        tween_ball,
        AnimateTweenY(
            Tween::new(
                Fixed::from_int(30),
                Fixed::from_int(250),
                800,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );
    //~focus-end

    //~focus-start
    let spring_ball = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 170,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        )
    };
    world.insert(
        spring_ball,
        SpringBall {
            spring: Spring::preset(Fixed::from_int(30), Fixed::from_int(250), SMOOTH).repeat(),
            x: Fixed::from_int(170),
        },
    );
    //~focus-end

    //~focus-start
    let elastic_ball = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(88, 166, 255),
            position: Position::Absolute,
            left: 300,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        )
    };
    world.insert(
        elastic_ball,
        SpringBall {
            spring: Spring::preset(Fixed::from_int(30), Fixed::from_int(250), BOUNCY).repeat(),
            x: Fixed::from_int(300),
        },
    );
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::ecs;
    app.add_system(ecs::System::new(
        "animate_tween_y",
        ecs::run_order::ANIMATION,
        AnimateTweenY::system(),
    ));
    app.add_system(spring_system::system());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
