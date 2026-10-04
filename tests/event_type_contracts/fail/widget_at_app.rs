use mirui::ecs::Component;
use mirui::input::event::contract::*;

struct Pad;
impl Component for Pad {}

struct Hit;
impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

fn app<E: Event>(_: EventKey<E>)
where
    E::Route: AppRoute,
    E::Source: AppEventSource,
{
}

fn main() {
    app(Hit::EVENT);
}
