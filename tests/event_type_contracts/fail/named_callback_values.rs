use mirui::event;

#[event]
fn owns_text(text: String) {
    let _ = text.len();
}

#[event]
fn raw_world(world: &mut mirui::ecs::World) {
    world.spawn_empty();
}

fn main() {}
