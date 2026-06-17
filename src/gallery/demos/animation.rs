extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
#[cfg(feature = "std")]
use crate::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui;

mirui_macros::animate!(AnimateX, |world, entity, value| {
    ui::set_position(world, entity, value, Fixed::from_int(60));
});

mirui_macros::animate!(AnimateColor, |world, entity, value| {
    let r = (value * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
    if let Some(style) = world.get_mut::<ui::Style>(entity) {
        style.set_bg_color(Color::rgb(r, 50, 255 - r));
    }
    world.insert(entity, ui::dirty::Dirty);
});

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let ball = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(255, 50, 100),
            border_radius: 20,
            position: Position::Absolute,
            left: 10,
            top: 60,
            width: 40,
            height: 40
        )
    };
    //~focus-end

    world.insert(
        ball,
        AnimateX(
            Tween::new(
                Fixed::from_int(10),
                Fixed::from_int(270),
                1200,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );
    world.insert(
        ball,
        AnimateColor(
            Tween::new(
                Fixed::ZERO,
                Fixed::ONE,
                2400,
                ease::ease_in_out_quad,
                PlayMode::Loop,
            )
            .into(),
        ),
    );
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::ecs;
    app.add_system(ecs::System::new(
        "animate_x",
        ecs::run_order::ANIMATION,
        AnimateX::system(),
    ));
    app.add_system(ecs::System::new(
        "animate_color",
        ecs::run_order::ANIMATION,
        AnimateColor::system(),
    ));
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
