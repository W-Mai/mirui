use mirui::system;

#[system]
fn escape_world() {
    let _: &mut mirui::ecs::World = __mirui_world;
}

fn main() {}
