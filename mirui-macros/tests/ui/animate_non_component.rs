use mirui::animate;

#[animate(component = NumberMotion)]
fn non_component(value: component!(u8)) {
    let _ = value;
}

fn main() {
    let _ = NumberMotion::system();
}
