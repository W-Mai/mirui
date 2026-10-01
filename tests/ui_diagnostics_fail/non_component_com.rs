use mirui::prelude::*;

struct PlainState;

#[compose]
fn panel(source: Entity) {
    let _ = com!(source, PlainState);

    ui! {
        View()
    };
}

fn main() {}
