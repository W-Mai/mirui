use mirui::prelude::*;
use mirui::ui::widgets::LazyList;

#[compose]
fn panel() {
    ui! {
        LazyList () on CreateCell(|_| true)
    };
}

fn main() {}
