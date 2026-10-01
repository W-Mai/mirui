use mirui_macros::timer;

#[timer(every = 10)]
fn tick(world: &mut mirui::ecs::World) {
    let _ = world;
}

fn main() {}
