use mirui::prelude::*;

#[compose]
fn panel() {
    let _ = mirui::core::reactive::with_world(|world| world.spawn_empty());
}

fn main() {
    let _ = mirui::core::reactive::with_world(|world| world.spawn_empty());
}
