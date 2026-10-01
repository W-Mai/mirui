use mirui::prelude::*;

#[model]
struct Counter {
    value: u8,
}

#[model]
impl Counter {}

#[compose]
fn panel(counter: model!(Counter)) {
    let _ = counter;
}

fn main() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    app.compose(root, |cx| panel(cx, Counter { value: 0 }));
}
