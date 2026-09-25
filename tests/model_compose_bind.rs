use mirui::prelude::*;

#[model]
struct Counter {
    count: u32,
}

#[model]
impl Counter {
    fn increment(&mut self) {
        self.count += 1;
    }

    fn count(&self) -> u32 {
        self.count
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

#[test]
fn compose_maps_bound_model_parameters_to_handles() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counter = app.add_model(Counter { count: 0 });
    app.compose(root, |cx| increment_panel(cx, counter.clone(), 3));
    app.compose(root, |cx| optional_panel(cx, Some(counter.clone())));
    assert_eq!(counter.count(), 4);
}
