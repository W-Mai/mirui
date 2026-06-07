use mirui::anim::{PlayMode, Tween, ease};
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

extern crate alloc;

mirui_macros::animate!(AnimateX, |world, entity, value| {
    mirui::widget::set_position(world, entity, value, Fixed::from_int(60));
});

mirui_macros::animate!(AnimateColor, |world, entity, value| {
    let r = (value * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
    if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
        style.set_bg_color(Color::rgb(r, 50, 255 - r));
    }
    world.insert(entity, mirui::widget::dirty::Dirty);
});

fn main() {
    let backend = SdlSurface::new("mirui - animation demo", 320, 180);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    app.add_system(mirui::ecs::System::new(
        "animate_x",
        mirui::ecs::run_order::ANIMATION,
        AnimateX::system(),
    ));
    app.add_system(mirui::ecs::System::new(
        "animate_color",
        mirui::ecs::run_order::ANIMATION,
        AnimateColor::system(),
    ));

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            width: Dimension::px(320),
            height: Dimension::px(180),
            ..Default::default()
        })
        .id();

    let ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (
            bg_color: Color::rgb(255, 50, 100),
            border_radius: 20,
            position: Position::Absolute,
            left: 10,
            top: 60,
            width: 40,
            height: 40
        ) {}
    };

    app.world.insert(
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
    app.world.insert(
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

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
