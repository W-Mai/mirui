use mirui::ecs::Component;
use mirui::input::event::contract::*;

struct Pad;
struct Dial;
impl Component for Pad {}
impl Component for Dial {}

struct Hit;
impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

fn entity<W: Component, E: Event>(_: EventKey<E>)
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<W>,
{
}

fn main() {
    entity::<Dial, Hit>(Hit::EVENT);
}
