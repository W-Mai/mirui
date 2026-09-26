use mirui::prelude::*;

#[model]
struct Counter {
    value: u8,
}

#[model]
impl Counter {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[compose(bind(counter))]
fn panel(counter: Counter) {
    counter.increment();
}

fn main() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    app.compose(root, |cx| panel(cx, Counter { value: 0 }));
}
