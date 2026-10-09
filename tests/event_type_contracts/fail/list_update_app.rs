use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::BindItem;

#[event]
fn bind_row(change: event!(BindItem)) {
    let _ = change;
}

fn main() {
    let mut app = App::headless(10, 10);
    app.on_event(BindItem::EVENT, bind_row::callback());
}
