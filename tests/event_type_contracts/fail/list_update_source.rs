use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::BindItem;
use mirui::ui::widgets::{LazyList, Slider};

#[event]
fn bind_row(change: event!(BindItem)) {
    let _ = change;
}

fn main() {
    let _ = Slider::build(0, 100).on_event(LazyList::BindItem, bind_row::callback());
}
