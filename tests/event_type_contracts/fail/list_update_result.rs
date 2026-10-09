use mirui::prelude::*;
use mirui::ui::widgets::LazyList;

#[compose]
fn panel() {
    ui! {
        LazyList () on BindItem(|_| true)
    };
}

fn main() {}
