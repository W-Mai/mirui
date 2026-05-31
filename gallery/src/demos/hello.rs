use crate::Setup;
use mirui::ecs::Entity;
use mirui::plugins::input_feedback::InputFeedbackPlugin;
use mirui::prelude::*;

pub fn build(setup: &mut Setup<'_>) -> Entity {
    setup.app.add_plugin(InputFeedbackPlugin::new());
    let world = &mut setup.app.world;
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 22, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(24),
            ..Default::default()
        })
        .id();
    ui! {
        :( parent: root world: world :)
        column (direction: FlexDirection::Column, grow: 1.0) {
            title (
                text: "mirui hello",
                bg_color: Color::rgb(38, 50, 70),
                text_color: Color::rgb(220, 220, 240),
                border_radius: 12,
                padding: Padding::all(16)
            ) {}
            spacer (height: 16) {}
            card (
                bg_color: Color::rgb(60, 80, 110),
                border_color: Color::rgb(120, 160, 220),
                border_width: 2,
                border_radius: 16,
                padding: Padding::all(20),
                grow: 1.0
            ) {
                msg (text: "tap to interact", text_color: Color::rgb(240, 240, 255)) {}
            }
        }
    };
    root
}
