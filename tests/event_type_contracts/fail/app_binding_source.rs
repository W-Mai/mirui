use mirui::prelude::*;

#[event]
fn tapped() -> bool {
    false
}

fn main() {
    let mut app = App::headless(16, 16);
    app.on_event(Tap::EVENT, tapped::callback());
}
