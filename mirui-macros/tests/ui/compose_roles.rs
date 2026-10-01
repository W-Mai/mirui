use mirui::ecs::Entity;
use mirui::prelude::*;
use mirui::ui::ViewRegistry;
use mirui::ui::widgets::{Button, Text};

#[model]
struct Counter {
    #[observe]
    value: u8,
}

#[model]
impl Counter {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[compose]
fn counter_text(counter: model!(Counter), label: &'static str) -> Entity {
    ui! {
        Text (text: ${ format!("{label}: {}", counter.value()) })
    }
}

#[compose]
fn optional_counter_text(counter: model!(optional Counter)) -> Entity {
    let label = counter
        .as_ref()
        .map_or("none".to_string(), |counter| counter.value().to_string());
    ui! {
        Text (label)
    }
}

#[compose]
fn generic_passthrough<T: Copy>(value: T) -> T {
    value
}

ui!(compose CounterCard {
    @@body
});

#[compose]
fn counter_panel(
    counter: model!(Counter),
    optional: model!(optional Counter),
    label: &'static str,
) -> Entity {
    ui! {
        Column () {
            counter_text (counter, label)
            counter_text (counter, "again")
            optional_counter_text (optional)
            optional_counter_text (optional)
            CounterCard () {
                @body {
                    counter_text (counter, "niche")
                }
            }
            Button (text: "increment") on Tap {
                counter.increment();
                if let Some(optional) = optional.as_ref() {
                    optional.increment();
                }
            }
        }
    }
}

fn main() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let mut views = ViewRegistry::default();
    views.insert(ui!(compose CounterCard));
    app.world.insert_resource(views);
    let counter = app.add_model(Counter { value: 1 });
    let optional = app.add_model(Counter { value: 2 });
    app.compose(root, |cx| {
        assert_eq!(generic_passthrough(cx, 7_u8), 7);
        counter_panel(cx, counter, Some(optional), "count");
    });
}
