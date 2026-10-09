use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::CreateCell;
use mirui::ui::widgets::{LazyList, Slider};

#[event]
fn bind_row(change: event!(CreateCell), node: Entity) -> Entity {
    let _ = change;
    node
}

fn main() {
    let node = Entity {
        id: 0,
        generation: 0,
    };
    let _ = Slider::build(0, 100).on_event(LazyList::CreateCell, bind_row::callback(node));
}
