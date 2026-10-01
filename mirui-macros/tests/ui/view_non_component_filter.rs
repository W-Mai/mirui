use mirui::view;

#[view]
fn paint(value: component!(u8)) {
    let _ = value;
}

fn main() {
    let _ = paint::view();
}
