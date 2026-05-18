use mirui::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
use mirui::app::App;
use mirui::ecs::{DeltaTimeMs, World};
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

extern crate alloc;

mirui_macros::animate!(AnimateTweenY, |world, entity, value| {
    mirui::widget::set_position(world, entity, Fixed::from_int(50), value);
});

struct SpringBall {
    spring: Spring,
    x: Fixed,
}

fn spring_system(world: &mut World) {
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
        mirui::widget::set_position(world, e, x, pos);
        if settled {
            if let Some(sb) = world.get_mut::<SpringBall>(e) {
                let new_target = if target.to_int() > 150 {
                    Fixed::from_int(30)
                } else {
                    Fixed::from_int(250)
                };
                sb.spring.retarget(new_target, None);
            }
        }
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - Tween vs Spring vs Elastic", 400, 300);
    let mut app = App::new(backend)
        .with_default_widgets()
        .with_default_systems();

    app.add_system(mirui::ecs::System::new(
        "animate_tween_y",
        mirui::ecs::run_order::ANIMATION,
        AnimateTweenY::system(),
    ));
    app.add_system(mirui::ecs::System::new(
        "spring",
        mirui::ecs::run_order::ANIMATION,
        spring_system,
    ));

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            width: Dimension::px(400),
            height: Dimension::px(300),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        l1 (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 25,
            top: 5,
            width: 70,
            height: 14,
            text: "Tween"
        ) {}
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        l2 (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 140,
            top: 5,
            width: 70,
            height: 14,
            text: "Spring"
        ) {}
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        l3 (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 270,
            top: 5,
            width: 70,
            height: 14,
            text: "Elastic"
        ) {}
    };

    // Tween ball (ease PingPong)
    let tween_ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tb (
            bg_color: Color::rgb(248, 81, 73),
            position: Position::Absolute,
            left: 50,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        ) {}
    };
    app.world.insert(
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

    // Spring ball (smooth, no bounce)
    let spring_ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        sb (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 170,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        ) {}
    };
    app.world.insert(
        spring_ball,
        SpringBall {
            spring: Spring::preset(Fixed::from_int(30), Fixed::from_int(250), SMOOTH).repeat(),
            x: Fixed::from_int(170),
        },
    );

    // Elastic ball (bouncy spring)
    let elastic_ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        eb (
            bg_color: Color::rgb(88, 166, 255),
            position: Position::Absolute,
            left: 300,
            top: 30,
            width: 20,
            height: 20,
            border_radius: 10
        ) {}
    };
    app.world.insert(
        elastic_ball,
        SpringBall {
            spring: Spring::preset(Fixed::from_int(30), Fixed::from_int(250), BOUNCY).repeat(),
            x: Fixed::from_int(300),
        },
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());

    loop {
        let frame_start = std::time::Instant::now();
        if app
            .poll_event()
            .map_or(false, |e| matches!(e, mirui::surface::InputEvent::Quit))
        {
            break;
        }
        app.systems.run_all(&mut app.world);
        app.render();
        let elapsed = frame_start.elapsed();
        let target = std::time::Duration::from_millis(33);
        if elapsed < target {
            std::thread::sleep(target - elapsed);
        }
    }
}
