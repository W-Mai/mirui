use mirui::ecs::Component;
use mirui::input::event::contract::*;

struct Pad;
impl Component for Pad {}

struct Invalid;
impl Event for Invalid {
    type Route = AppNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

fn main() {}
