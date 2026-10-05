use mirui::__macro_support::NodeContext;
use mirui::input::event::input::Wheel;
use mirui::input::event::lifecycle::Started;

#[mirui::event]
fn wheel(input: event!(Wheel)) {
    let _ = input;
}

#[mirui::event]
fn started(input: event!(Started)) {
    let _ = input;
}

fn main() {
    let _ = wheel::callback::<NodeContext>();
    let _ = started::callback::<NodeContext>();
}
