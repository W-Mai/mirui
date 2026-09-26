use mirui::prelude::*;

#[component(bind(items))]
struct BadComponent {
    items: Vec<u8>,
}

#[compose(bind(label))]
fn bad_panel(label: String) {
    let _ = label;
}

fn main() {}
