use mirui::prelude::*;

#[event]
fn started(node: entity!()) {
    let _ = node;
}

fn main() {
    let mut app = App::headless(16, 16);
    app.on_event(Started::EVENT, started::callback());
}
