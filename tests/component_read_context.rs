use std::cell::Cell;
use std::rc::Rc;

use mirui::input::event::GestureHandler;
use mirui::input::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::ui::widgets::Button;

#[component]
struct Marker {
    value: u32,
}

#[compose]
fn marker_panel(source: Entity, seen: Rc<Cell<u32>>) {
    assert_eq!(
        cx.component::<Marker>(source).map(|item| item.value),
        Some(7)
    );
    assert_eq!(com!(source, Marker).map(|item| item.value), Some(7));

    ui! {
        Button(text: "READ") on Tap {
            let marker = mirui::com!(target, Marker).expect("target marker should exist");
            assert_eq!(ctx.component::<Marker>(target).map(|item| item.value), Some(11));
            seen.set(marker.value);
        }
    };
}

#[test]
fn compose_and_event_component_reads_share_the_same_world() {
    let mut app = App::headless(120, 48);
    let root = app.spawn_root().id();
    let target = app.world.spawn_empty();
    app.world.insert(target, Marker { value: 7 });
    let seen = Rc::new(Cell::new(0));

    app.compose(root, |cx| marker_panel(cx, target, seen.clone()));

    let button = app.world.query::<Button>().iter().next().expect("button").0;
    app.world.insert(button, Marker { value: 11 });
    let event = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: button,
    };
    assert_eq!(
        GestureHandler::trigger(&mut app.world, button, &event),
        Some(true)
    );
    assert_eq!(seen.get(), 11);
}
