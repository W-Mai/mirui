use mirui::ecs::Component;
use mirui::input::event::contract::*;

struct Pad;
impl Component for Pad {}

struct Hit {
    strength: u8,
}

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

impl Pad {
    const HIT: EventKey<Hit> = Hit::EVENT;
}

struct Key;
impl Event for Key {
    type Route = InputFallback;
    type Source = InputSource;
    type Options = ();
}

fn entity<W: Component, E: Event>(_: EventKey<E>)
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<W>,
{
}

fn app<E: Event>(_: EventKey<E>)
where
    E::Route: AppRoute,
    E::Source: AppEventSource,
{
}

fn notify<E: Event, R: EventResult<E::Route>>(_: EventKey<E>, result: R) {
    let _ = result.into_decision();
}

fn main() {
    entity::<Pad, Hit>(Pad::HIT);
    entity::<Pad, Key>(Key::EVENT);
    app(Key::EVENT);
    notify(Pad::HIT, ());
    let hit = Hit { strength: 7 };
    assert_eq!(hit.strength, 7);
}
