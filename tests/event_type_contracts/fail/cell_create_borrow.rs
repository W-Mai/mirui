use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::CreateCell;
use mirui::ui::widgets::{LazyList, Text};

#[event]
fn create(event: event!(CreateCell), texts: query!(mut Text)) -> Entity {
    let _ = (event, texts);
    ui! {
        Text ("cell")
    }
}

#[compose]
fn panel() {
    ui! {
        LazyList () on CreateCell(create::callback())
    };
}
fn main() {}
