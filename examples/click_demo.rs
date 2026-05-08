use std::sync::{Arc, Mutex};

use mirui::app::App;
use mirui::backend::Backend;
use mirui::backend::sdl::SdlBackend;
use mirui::ecs::Entity;
use mirui::event::{EventHandler, WidgetEvent};
use mirui::layout::*;
use mirui::types::{Color, Dimension};
use mirui::widget::Style;
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlBackend::new("mirui - click demo", 480, 320);
    let mut app = App::new(backend);

    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
    ];

    // Shared state for toggling colors
    let toggle_states: Arc<Mutex<[bool; 3]>> = Arc::new(Mutex::new([false; 3]));

    let mut children: Vec<Entity> = Vec::new();
    for (i, &color) in colors.iter().enumerate() {
        let toggle = toggle_states.clone();
        let child = WidgetBuilder::new(&mut app.world)
            .bg_color(color)
            .layout(LayoutStyle {
                width: Dimension::px(120),
                height: Dimension::px(80),
                ..Default::default()
            })
            .id();

        app.world.insert(
            child,
            EventHandler::new(move |entity, event| {
                if let WidgetEvent::Click { .. } = event {
                    let mut states = toggle.lock().unwrap();
                    states[i] = !states[i];
                    // We can't mutate world here directly, but we print to show it works
                    let state = if states[i] { "ON" } else { "OFF" };
                    println!("Widget {entity:?} clicked! State: {state}");
                }
            }),
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

    // Custom run loop to update colors based on toggle state
    app.render();
    loop {
        match app.poll_event() {
            Some(mirui::backend::InputEvent::Quit) => break,
            Some(event) => {
                if let Some(root) = app.root {
                    let info = app.backend.display_info();
                    mirui::event::dispatch::dispatch(
                        &app.world,
                        root,
                        &event,
                        info.width,
                        info.height,
                    );
                }

                // Update colors based on toggle state
                let states = toggle_states.lock().unwrap();
                for (i, &child) in children.iter().enumerate() {
                    if let Some(style) = app.world.get_mut::<Style>(child) {
                        style.bg_color = Some(if states[i] {
                            Color::rgb(210, 168, 255) // toggled = purple
                        } else {
                            colors[i]
                        });
                    }
                }
                drop(states);

                app.render();
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }
}
