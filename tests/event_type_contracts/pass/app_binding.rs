use mirui::prelude::*;

#[event]
fn lifecycle() {}

#[event]
fn fallback() -> bool {
    false
}

#[event]
fn deferred(commands: commands!()) -> bool {
    commands.insert_resource(7_u32);
    true
}

fn main() {
    const CAPACITY: EventCapacity =
        EventCapacity::new().commands(CommandCapacity::new().insert_resource::<u32>(1));
    let mut app = App::headless(16, 16);
    app.with_event_capacity(CAPACITY)
        .on_event(Started::EVENT, lifecycle::callback())
        .on_event(Suspended::EVENT, lifecycle::callback())
        .on_event(Resumed::EVENT, lifecycle::callback())
        .on_event(MemoryWarning::EVENT, lifecycle::callback())
        .on_event(HostInteraction::EVENT, lifecycle::callback())
        .on_event(Quit::EVENT, lifecycle::callback())
        .on_event(Key::EVENT, deferred::callback())
        .on_event(CharInput::EVENT, fallback::callback())
        .on_event(Wheel::EVENT, fallback::callback())
        .on_event(Rotary::EVENT, fallback::callback());
}
