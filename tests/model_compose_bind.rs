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

struct MoveOnly(u8);

type CounterAlias = Counter;

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

#[compose(bind(counter, optional))]
fn alias_panel(counter: CounterAlias, optional: Option<Option<crate::Counter>>, raw: Counter) {
    assert_eq!(raw.count, 17);
    counter.increment();
    if let Some(Some(optional)) = optional {
        optional.increment();
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

#[compose(bind(counter))]
fn child_counter_button(counter: Counter) -> mirui::ecs::Entity {
    ui! { Button(text: "CHILD") on Tap { counter.increment(); } }
}

#[compose]
fn consume_plain_child(value: MoveOnly) {
    assert_eq!(value.0, 7);
}

#[compose(bind(counter))]
fn parent_with_bound_children(counter: Counter, plain: MoveOnly) {
    ui!(consume_plain_child(plain));
    ui!(child_counter_button(counter));
    ui!(child_counter_button(counter));
    ui! { Button(text: "PARENT") on Tap { counter.increment(); } };
}

#[compose(bind(counter))]
fn qualified_ui_panel(counter: Counter) {
    mirui::ui! { Button(text: "MIRUI") on Tap { counter.increment(); } };
    crate::ui! { Button(text: "CRATE") on Tap { counter.increment(); } };
    ::mirui::ui!(child_counter_button(counter));
    mirui::ui!(child_counter_button(counter));
}

#[compose(bind(counter))]
fn tree_with_bound_children(counter: Counter) -> mirui::ecs::Entity {
    ui! {
        Column {
            child_counter_button(counter)
            child_counter_button(counter)
            Button(text: "PARENT") on Tap { counter.increment(); }
        }
    }
}

#[compose(bind(counter))]
fn optional_counter_button(counter: Option<Counter>) -> mirui::ecs::Entity {
    ui! {
        Button(text: "OPTIONAL") on Tap {
            if let Some(counter) = counter.as_ref() {
                counter.increment();
            }
        }
    }
}

#[compose(bind(counter))]
fn tree_with_optional_children(counter: Option<Counter>) -> mirui::ecs::Entity {
    ui! {
        Column {
            optional_counter_button(counter)
            optional_counter_button(counter)
        }
    }
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
    app.compose(root, |cx| {
        alias_panel(
            cx,
            counter.clone(),
            Some(Some(counter.clone())),
            Counter { count: 17 },
        )
    });
    assert_eq!(counter.count(), 6);
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

#[test]
fn bound_model_is_shared_across_child_compose_calls() {
    let mut app = App::headless(320, 120);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| {
        parent_with_bound_children(cx, counter.clone(), MoveOnly(7))
    });

    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 3);
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
    assert_eq!(counter.count(), 3);
}

#[test]
fn bound_model_is_shared_across_tree_child_compose_calls() {
    let mut app = App::headless(320, 120);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| {
        tree_with_bound_children(cx, counter.clone());
    });

    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 3);
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
    assert_eq!(counter.count(), 3);
}

#[test]
fn bound_optional_model_is_shared_across_tree_children() {
    let mut app = App::headless(320, 120);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| {
        tree_with_optional_children(cx, Some(counter.clone()));
    });

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
    assert_eq!(counter.count(), 2);
}

#[test]
fn qualified_ui_paths_share_bound_models_in_trees_and_children() {
    let mut app = App::headless(320, 120);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| qualified_ui_panel(cx, counter.clone()));

    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 4);
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
    assert_eq!(counter.count(), 4);
}
