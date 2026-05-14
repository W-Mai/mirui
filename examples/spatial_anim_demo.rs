use mirui::anim::{Animation, PlayMode, ease};
use mirui::app::App;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

extern crate alloc;

mirui_macros::animation!(AnimateTemporal, |world, entity, value| {
    mirui::widget::set_position(world, entity, value, Fixed::from_int(40));
});

mirui_macros::animation!(AnimateSpatial, |world, entity, value| {
    mirui::widget::set_position(world, entity, value, Fixed::from_int(120));
});

fn main() {
    let backend = SdlSurface::new("mirui - spatial vs temporal animation", 400, 200);
    let mut app = App::new(backend);

    app.add_system(mirui::anim::sync_delta_time_ms);
    app.add_system(AnimateTemporal::system());
    app.add_system(AnimateSpatial::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            width: Dimension::px(400),
            height: Dimension::px(200),
            ..Default::default()
        })
        .id();

    // Label area
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        label_temporal (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 10,
            top: 20,
            width: 80,
            height: 16,
            text: "Temporal"
        ) {}
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        label_spatial (
            bg_color: Color::rgb(40, 40, 50),
            position: Position::Absolute,
            left: 10,
            top: 100,
            width: 80,
            height: 16,
            text: "Spatial"
        ) {}
    };

    // Temporal ball (top track)
    let temporal_ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        temporal_ball (
            bg_color: Color::rgb(248, 81, 73),
            position: Position::Absolute,
            left: 20,
            top: 40,
            width: 30,
            height: 30,
            border_radius: 15
        ) {}
    };

    app.world.insert(
        temporal_ball,
        AnimateTemporal(Animation::new(
            Fixed::from_int(20),
            Fixed::from_int(350),
            1500,
            ease::ease_in_out_cubic,
            PlayMode::Loop,
        )),
    );

    // Spatial ball (bottom track)
    let spatial_ball = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        spatial_ball (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 20,
            top: 120,
            width: 30,
            height: 30,
            border_radius: 15
        ) {}
    };

    app.world.insert(
        spatial_ball,
        AnimateSpatial(Animation::spatial(
            Fixed::from_int(20),
            Fixed::from_int(350),
            1500,
            ease::ease_in_out_cubic,
            PlayMode::Loop,
            660,
        )),
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
