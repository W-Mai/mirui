use mirui::prelude::*;

#[compose]
fn panel<T>(source: Entity) {
    let _ = com!(source, T);

    ui! {
        View()
    };
}

fn main() {}
