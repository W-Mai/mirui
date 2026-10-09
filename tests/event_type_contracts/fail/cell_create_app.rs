use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::CreateCell;

#[event]
fn bind_row(change: event!(CreateCell), node: Entity) -> Entity {
    let _ = change;
    node
}

fn main() {
    let mut app = App::headless(10, 10);
    let root = app.spawn_root().id();
    app.on_event(CreateCell::EVENT, bind_row::callback(root));
}
