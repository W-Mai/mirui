#![deny(warnings)]

use mirui::__macro_support::{
    CommandCapacity, EntityContext, EventListeners, ListenerSelector, NodeContext,
    NotificationCapacity, NotificationQueue,
};
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

struct Hits(u8);

#[mirui::event]
fn hit_event(hit: event!(Hit), hits: res!(mut Hits)) {
    hits.0 += hit.strength;
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

    let mut world = mirui::ecs::World::new();
    let node = world.spawn_empty();
    world.insert(node, Pad);
    world.insert_resource(Hits(0));
    let listeners = EventListeners::new(&mut world);
    listeners
        .reserve(mirui::__macro_support::ListenerCapacity {
            listeners: 2,
            snapshot_steps: 2,
        })
        .unwrap();
    listeners
        .install_entity(
            &mut world,
            node,
            ListenerSelector::new(Pad::HIT, |(), _| true),
            hit_event::callback::<EntityContext<Pad>>(),
            CommandCapacity::default(),
        )
        .unwrap();
    listeners
        .install_entity(
            &mut world,
            node,
            ListenerSelector::new(Pad::HIT, |(), _| true),
            hit_event::callback::<NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 1,
            payload_bytes: 1,
            payload_alignment: 1,
        })
        .unwrap();
    queue.push_entity(node, hit).ok().unwrap();
    listeners
        .flush_notifications(&mut world, &queue, 1)
        .unwrap();
    assert_eq!(world.resource::<Hits>().unwrap().0, 14);
}
