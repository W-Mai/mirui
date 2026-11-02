use mirui::prelude::*;

#[compose]
fn number(value: prop!(u32)) -> u32 {
    value.get()
}

fn main() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            number (${ String::from("wrong type") })
        }
    });
}
