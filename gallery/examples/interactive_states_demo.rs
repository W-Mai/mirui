use mirui::app::App;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::UserState;
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

struct ToggleErrored;

fn toggle_errored_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    if matches!(world.get::<UserState>(entity), Some(UserState::Errored)) {
        world.remove::<UserState>(entity);
    } else {
        world.insert(entity, UserState::Errored);
    }
    world.insert(entity, Dirty);
    true
}

fn main() {
    let backend = SdlSurface::new("mirui - interactive states demo", 480, 320);
    let mut app = App::new(backend)
        .with_default_widgets()
        .with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            padding: Padding::all(24),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        row (
            direction: FlexDirection::Row,
            grow: 1.0
        ) {
            enabled (
                bg_color: Color::rgb(63, 185, 80),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Hover / Press me"
            ) {}
            errored (
                bg_color: Color::rgb(248, 81, 73),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Tap to toggle Errored"
            ) [
                ToggleErrored,
                GestureHandler {
                    on_gesture: toggle_errored_handler,
                },
            ] {}
            disabled (
                bg_color: Color::rgb(88, 166, 255),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Disabled"
            ) [
                UserState::Disabled,
            ] {}
        }
    };

    app.set_root(root);
    app.run();
}
