use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::GestureHandler;
use mirui::input::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::types::Fixed;
use mirui::ui::widgets::Button;
use mirui::ui::widgets::Text;

#[model]
struct Counter {
    #[observe]
    count: u32,
}

#[model]
impl Counter {
    fn increment(&mut self) {
        self.count += 1;
    }
}

#[compose(bind(counter))]
fn increment_panel(counter: Counter, unbound: u32) {
    for _ in 0..unbound {
        counter.increment();
    }
}

#[compose(bind(counter))]
fn optional_panel(counter: Option<Counter>) {
    if let Some(counter) = counter {
        counter.increment();
    }
}

#[compose(bind(counter))]
fn two_button_panel(counter: Counter) {
    ui! {
        Row {
            Button(text: "FIRST") on Tap { counter.increment(); }
            Button(text: "SECOND") on Tap { counter.increment(); }
            Text(text: ${ counter.count().to_string() })
        }
    };
}

#[compose(bind(counter))]
fn mixed_capture_panel(counter: Counter) {
    ui! {
        Row {
            Button(text: "STATIC") on Tap { let _ = 1; }
            Button(text: "UPDATE") on Tap { counter.increment(); }
        }
    };
}

#[test]
fn unrelated_callbacks_do_not_capture_a_bound_model() {
    let mut app = App::headless(160, 48);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| mixed_capture_panel(cx, counter.clone()));
    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 2);
    for button in buttons {
        let event = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: button,
        };
        GestureHandler::trigger(&mut app.world, button, &event);
    }
    assert_eq!(counter.count(), 1);
}

#[test]
fn compose_maps_bound_model_parameters_to_handles() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| increment_panel(cx, counter.clone(), 3));
    app.compose(root, |cx| optional_panel(cx, Some(counter.clone())));
    assert_eq!(counter.count(), 4);
}

#[test]
fn bound_model_is_shared_across_generated_callbacks() {
    let mut app = App::headless(320, 120);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| two_button_panel(cx, counter.clone()));
    let labels: Vec<_> = app.world.query::<Text>().collect();
    let label = *labels
        .iter()
        .find(|entity| app.world.get::<Text>(**entity).unwrap().resolve(&app.world) == "0")
        .expect("counter label");
    assert_eq!(
        app.world.get::<Text>(label).unwrap().resolve(&app.world),
        "0"
    );
    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 2);
    for button in buttons {
        let event = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: button,
        };
        assert_eq!(
            GestureHandler::trigger(&mut app.world, button, &event),
            Some(true)
        );
    }
    flush_signal_dirty(&mut app.world);
    assert_eq!(counter.count(), 2);
    assert_eq!(
        app.world.get::<Text>(label).unwrap().resolve(&app.world),
        "2"
    );
}
