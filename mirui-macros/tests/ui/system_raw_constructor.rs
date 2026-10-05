use mirui::ecs::{CommandError, System, World};

fn raw(_: &mut World) -> Result<(), CommandError> {
    Ok(())
}

fn main() {
    let _ = System::new("raw", 0, raw);
}
