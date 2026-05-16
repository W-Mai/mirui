use mirui::app::App;
use mirui::components::slider::Slider;
use mirui::components::switch::Switch;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlSurface::new("mirui - slider & switch", 320, 200);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(200),
            padding: Padding {
                top: Dimension::px(30),
                left: Dimension::px(20),
                right: Dimension::px(20),
                bottom: Dimension::px(30),
            },
            ..Default::default()
        })
        .id();

    let slider = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        slider (width: 200, height: 16) [
            Slider::new(Fixed::ZERO, Fixed::from_int(100)),
        ] {}
    };
    if let Some(s) = app.world.get_mut::<Slider>(slider) {
        s.value = Fixed::from_int(50);
    }

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        switch (width: 50, height: 26) [
            Switch::new(),
        ] {}
    };

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
