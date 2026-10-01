use mirui::ecs::{System, World};

fn raw(_: &mut World) {}

fn main() {
    let _ = System::new("raw", 0, raw);
}
