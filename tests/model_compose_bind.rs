#![deny(unused_variables)]

use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::GestureHandler;
use mirui::input::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::types::Fixed;
use mirui::ui::widgets::Button;
use mirui::ui::widgets::Text;
use std::cell::Cell;
use std::format as fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[model]
struct Counter {
    #[observe]
    count: u32,
}

struct MoveOnly(u8);

struct CounterField {
    counter: u8,
}

type CounterAlias = Counter;

#[model]
impl Counter {
    fn increment(&mut self) {
        self.count += 1;
    }
}

impl core::fmt::Display for CounterHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.count())
    }
}

#[compose]
fn increment_panel(counter: model!(Counter), unbound: u32) {
    for _ in 0..unbound {
        counter.increment();
    }
}

#[compose]
fn optional_panel(counter: model!(optional Counter)) {
    if let Some(counter) = counter {
        counter.increment();
    }
}

#[compose]
fn alias_panel(
    counter: model!(CounterAlias),
    optional: model!(optional Option<crate::Counter>),
    raw: Counter,
) {
    assert_eq!(raw.count, 17);
    counter.increment();
    if let Some(Some(optional)) = optional {
        optional.increment();
    }
}

#[compose]
fn two_button_panel(counter: model!(Counter)) {
    ui! {
        Row {
            Button(text: "FIRST") on Tap { counter.increment(); }
            Button(text: "SECOND") on Tap { counter.increment(); }
            Text(text: ${ counter.count().to_string() })
        }
    };
}

#[compose]
fn mixed_capture_panel(counter: model!(Counter)) {
    ui! {
        Row {
            Button(text: "STATIC") on Tap { let _ = 1; }
            Button(text: "UPDATE") on Tap { counter.increment(); }
        }
    };
}

#[compose]
fn scoped_capture_panel(counter: model!(Counter)) {
    assert_eq!(counter.count(), 13);
    ui! {
        Row {
            Button(text: "SHADOW") on Tap {
                let counter = 7u8;
                let _ = counter;
            }
            Button(text: "FIELD") on Tap {
                let value = CounterField { counter: 9 };
                let _ = value.counter;
            }
            Button(text: "INITIALIZER") on Tap {
                let counter = counter.count();
                let _ = counter;
            }
            Button(text: "CLOSURE") on Tap {
                let read = |counter: u8| counter + 1;
                let _ = read(4);
            }
            Button(text: "IF LET") on Tap {
                if let Some(counter) = Some(5u8) {
                    let _ = counter;
                }
            }
            Button(text: "MATCH") on Tap {
                match Some(6u8) {
                    Some(counter) => { let _ = counter; }
                    None => { panic!("missing value"); }
                }
            }
            Button(text: "MATCHES") on Tap {
                let _ = matches!(7u8, counter if counter == 7);
            }
            Button(text: "FREE") on Tap {
                let _ = format!("{}", counter.count());
                let _ = format_args!("{}", counter.count()).to_string();
            }
            Button(text: "FREE AGAIN") on Tap {
                let _ = format!("{}", counter.count());
            }
        }
    };
}

#[compose]
fn implicit_format_capture_panel(counter: model!(Counter)) {
    ui! {
        Row {
            Button(text: "IMPLICIT") on Tap { let _ = format!("{counter}"); }
            Button(text: "SPECIFIER") on Tap { let _ = format!("{counter:>4}"); }
            Button(text: "ARGS") on Tap { let _ = format_args!("{counter}").to_string(); }
            Button(text: "ABSOLUTE") on Tap { let _ = ::std::format!("{counter}"); }
            Button(text: "ALIAS") on Tap { let _ = fmt!("{}", counter.count()); }
            Button(text: "ESCAPED") on Tap { let _ = format!("{{counter}} {value}", value = 1); }
            Button(text: "NAMED") on Tap { let _ = format!("{counter}", counter = 7); }
            Button(text: "NAMED VALUE") on Tap { let _ = format!("{counter}", counter = counter.count()); }
            Button(text: "LOCAL") on Tap {
                let counter = 8;
                let _ = format!("{counter}");
            }
            Text(text: ${ format!("{counter}") })
        }
    };
}

#[compose]
fn child_counter_button(counter: model!(Counter)) -> mirui::ecs::Entity {
    ui! { Button(text: "CHILD") on Tap { counter.increment(); } }
}

#[compose]
fn consume_plain_child(value: MoveOnly) {
    assert_eq!(value.0, 7);
}

#[compose]
fn parent_with_bound_children(counter: model!(Counter), plain: MoveOnly) {
    ui!(consume_plain_child(plain));
    ui!(child_counter_button(counter));
    ui!(child_counter_button(counter));
    ui! { Button(text: "PARENT") on Tap { counter.increment(); } };
}

#[compose]
fn qualified_ui_panel(counter: model!(Counter)) {
    mirui::ui! { Button(text: "MIRUI") on Tap { counter.increment(); } };
    crate::ui! { Button(text: "CRATE") on Tap { counter.increment(); } };
    ::mirui::ui!(child_counter_button(counter));
    mirui::ui!(child_counter_button(counter));
}

#[compose]
fn tree_with_bound_children(counter: model!(Counter)) -> mirui::ecs::Entity {
    ui! {
        Column {
            child_counter_button(counter)
            child_counter_button(counter)
            Button(text: "PARENT") on Tap { counter.increment(); }
        }
    }
}

#[compose]
fn optional_counter_button(counter: model!(optional Counter)) -> mirui::ecs::Entity {
    ui! {
        Button(text: "OPTIONAL") on Tap {
            if let Some(counter) = counter.as_ref() {
                counter.increment();
            }
        }
    }
}

#[compose]
fn tree_with_optional_children(counter: model!(optional Counter)) -> mirui::ecs::Entity {
    ui! {
        Column {
            optional_counter_button(counter)
            optional_counter_button(counter)
        }
    }
}

#[compose]
fn checked_panel<T: mirui::core::model::BindType>(counter: model!(T), entered: &Cell<bool>) {
    entered.set(true);
}

#[compose]
fn checked_counter_panel(counter: model!(Counter), entered: &Cell<bool>) {
    entered.set(true);
}

#[compose]
fn checked_optional_panel(
    first: model!(CounterAlias),
    second: model!(optional Option<Counter>),
    entered: &Cell<bool>,
) {
    first.increment();
    if let Some(Some(second)) = second {
        second.increment();
    }
    entered.set(true);
}

#[test]
fn declared_models_are_all_validated_before_composition_side_effects() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let first = app.add_model(Counter { count: 0 });
    let mut other = App::headless(32, 32);
    let foreign = other.add_model(Counter { count: 0 });
    let entered = Cell::new(false);
    let result = catch_unwind(AssertUnwindSafe(|| {
        app.compose(root, |cx| {
            checked_optional_panel(cx, first.clone(), Some(Some(foreign.clone())), &entered)
        });
    }));
    assert!(result.is_err());
    assert!(!entered.get());
    assert_eq!(first.count(), 0);
    assert_eq!(foreign.count(), 0);
}

#[test]
fn ended_registration_is_rejected_even_when_the_model_is_unused() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let ended = {
        let mut other = App::headless(32, 32);
        other.add_model(Counter { count: 0 })
    };
    let entered = Cell::new(false);
    let result = catch_unwind(AssertUnwindSafe(|| {
        app.compose(root, |cx| checked_counter_panel(cx, ended, &entered));
    }));
    assert!(result.is_err());
    assert!(!entered.get());
}

#[test]
fn optional_model_validation_preserves_absent_values_and_instance_identity() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let first = app.add_model(Counter { count: 0 });
    let second = app.add_model(Counter { count: 10 });
    let entered = Cell::new(false);
    app.compose(root, |cx| {
        checked_optional_panel(cx, first.clone(), None, &entered);
        checked_optional_panel(cx, first.clone(), Some(None), &entered);
        checked_optional_panel(cx, first.clone(), Some(Some(second.clone())), &entered);
    });
    assert!(entered.get());
    assert_eq!(first.count(), 3);
    assert_eq!(second.count(), 11);
}

#[test]
fn model_validation_uses_the_supplied_scope_and_restores_nested_owner() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let first = app.add_model(Counter { count: 0 });
    let mut other = App::headless(32, 32);
    let other_root = other.spawn_root().id();
    let second = other.add_model(Counter { count: 10 });
    let entered = Cell::new(false);
    let mut scope = mirui::ui::scope::UiScope::new(&mut app.world, root);
    let _outer = mirui::ui::scope::UiScope::new(&mut other.world, other_root);
    checked_counter_panel(&mut scope, first, &entered);
    assert!(entered.get());
    second.increment();
    let result = catch_unwind(AssertUnwindSafe(|| {
        checked_counter_panel(&mut scope, second.clone(), &entered);
    }));
    assert!(result.is_err());
    second.increment();
    assert_eq!(second.count(), 12);
}

#[test]
fn shared_reactive_values_keep_their_own_access_rules() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let mut other = App::headless(32, 32);
    let other_root = other.spawn_root().id();
    let signal = Signal::new(5_u32);
    let computed = {
        let _scope = mirui::ui::scope::UiScope::new(&mut other.world, other_root);
        Computed::new(|| 7_u32)
    };
    let entered = Cell::new(false);
    app.compose(root, |cx| {
        checked_panel::<Signal<u32>>(cx, signal, &entered);
        checked_panel::<Computed<u32>>(cx, computed, &entered);
    });
    assert!(entered.get());
}

#[test]
fn declared_model_validation_does_not_allocate() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    let entered = Cell::new(false);
    let mut scope = mirui::ui::scope::UiScope::new(&mut app.world, root);
    let count = tracked_allocations(|| {
        for _ in 0..8 {
            checked_counter_panel(&mut scope, counter.clone(), &entered);
        }
    });
    assert_eq!(count, 0);
    assert!(entered.get());
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
fn generated_callbacks_share_only_free_model_references() {
    let mut app = App::headless(160, 48);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 13 });
    app.compose(root, |cx| scoped_capture_panel(cx, counter.clone()));
    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 9);
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
    assert_eq!(counter.count(), 13);
}

#[test]
fn generated_callbacks_share_implicit_format_captures() {
    let mut app = App::headless(160, 48);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 13 });
    app.compose(root, |cx| {
        implicit_format_capture_panel(cx, counter.clone())
    });
    let buttons: Vec<_> = app.world.query::<Button>().collect();
    assert_eq!(buttons.len(), 9);
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
    assert_eq!(counter.count(), 13);
    counter.increment();
    flush_signal_dirty(&mut app.world);
    let labels: Vec<_> = app.world.query::<Text>().collect();
    assert!(
        labels
            .iter()
            .any(|&entity| { app.world.get::<Text>(entity).unwrap().resolve(&app.world) == "14" })
    );
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
