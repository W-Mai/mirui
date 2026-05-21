use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::UserState;
use mirui::widget::dirty::Dirty;

struct ToggleTarget(Entity);

struct ClickCount(u32);

fn toggle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let target = world.get::<ToggleTarget>(entity).map(|t| t.0);
    let Some(target) = target else { return false };
    if matches!(world.get::<UserState>(target), Some(UserState::Disabled)) {
        world.remove::<UserState>(target);
    } else {
        world.insert(target, UserState::Disabled);
    }
    world.insert(target, Dirty);
    true
}

fn count_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let next = {
        let Some(c) = world.get_mut::<ClickCount>(entity) else {
            return false;
        };
        c.0 += 1;
        c.0
    };
    let buf = format!("Clicked: {next}");
    if let Some(t) = world.get_mut::<mirui::components::text::Text>(entity) {
        t.0 = buf.into_bytes();
    }
    world.insert(entity, Dirty);
    true
}

fn main() {
    let backend = SdlSurface::new("mirui - disabled demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

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

    let row_e = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        row (
            direction: FlexDirection::Row,
            grow: 1.0
        ) {
            counted (
                bg_color: Color::rgb(63, 185, 80),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Tap me to bump count"
            ) [
                ClickCount(0),
                GestureHandler {
                    on_gesture: count_handler,
                },
            ] {}
            target (
                bg_color: Color::rgb(248, 81, 73),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Disabled target"
            ) [
                ClickCount(0),
                GestureHandler {
                    on_gesture: count_handler,
                },
            ] {}
        }
    };
    let target = app
        .world
        .get::<mirui::widget::Children>(row_e)
        .and_then(|c| c.0.get(1).copied())
        .expect("target child of row");

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        toggle (
            height: 40,
            bg_color: Color::rgb(88, 166, 255),
            border_radius: 8,
            padding: Padding::all(8),
            text: "Toggle Disabled on right card"
        ) [
            ToggleTarget(target),
            GestureHandler {
                on_gesture: toggle_handler,
            },
        ] {}
    };

    app.set_root(root);
    app.run();
}
