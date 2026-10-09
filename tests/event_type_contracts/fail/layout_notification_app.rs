use mirui::prelude::*;

#[event]
fn resized(change: event!(SizeChanged)) {
    let _ = change;
}

fn main() {
    let mut app = App::headless(10, 10);
    app.on_event(SizeChanged::EVENT, resized::callback());
}
