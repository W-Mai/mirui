use mirui::event;
use mirui::input::event::input::Key;

struct State(u32);

fn retain_key(_: &'static Key) {}
fn retain_state(_: &'static State) {}

#[event]
fn escapes_key(input: event!(Key)) {
    retain_key(input);
}

#[event]
fn mutates_key(input: event!(Key)) {
    input.code = 1;
}

#[event]
fn escapes_resource(state: res!(State)) {
    retain_state(&state);
}

fn main() {}
