use mirui::ecs::Component;
use mirui::input::event::contract::{EntitySourceFor, WidgetSource};

struct Pad;
struct Dial;
impl Component for Pad {}
impl Component for Dial {}

impl EntitySourceFor<Dial> for WidgetSource<Pad> {
    fn present(_: &mirui::ecs::World, _: mirui::ecs::Entity) -> bool {
        true
    }
}

fn main() {}
