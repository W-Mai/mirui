use mirui::app::App;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::Style;
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;

struct Toggle {
    on: bool,
    base: Color,
    accent: Color,
}

fn toggle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        let new_color = {
            let Some(t) = world.get_mut::<Toggle>(entity) else {
                return false;
            };
            t.on = !t.on;
            if t.on { t.accent } else { t.base }
        };
        if let Some(style) = world.get_mut::<Style>(entity) {
            style.bg_color = Some(new_color);
        }
        world.insert(entity, Dirty);
        true
    } else {
        false
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - click demo", 480, 320);
    let mut app = App::new(backend);

    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
    ];
    let accent = Color::rgb(210, 168, 255);

    let mut children: Vec<Entity> = Vec::new();
    for &base in colors.iter() {
        let child = WidgetBuilder::new(&mut app.world)
            .bg_color(base)
            .layout(LayoutStyle {
                width: Dimension::px(120),
                height: Dimension::px(80),
                ..Default::default()
            })
            .id();

        app.world.insert(
            child,
            Toggle {
                on: false,
                base,
                accent,
            },
        );
        app.world.insert(
            child,
            GestureHandler {
                on_gesture: toggle_handler,
            },
        );
        children.push(child);
    }

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Dimension::px(480),
            height: Dimension::px(320),
            padding: Padding {
                top: 20.into(),
                right: 20.into(),
                bottom: 20.into(),
                left: 20.into(),
            },
            ..Default::default()
        })
        .child(children[0])
        .child(children[1])
        .child(children[2])
        .id();

    app.set_root(root);
    app.run();
}
