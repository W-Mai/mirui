#![deny(warnings)]

use mirui::__macro_support::{
    CommandCapacity, CommandError, EntityContext, EventListeners, ListenerSelector, NodeContext,
    NotificationCapacity, flush_notifications, prepare_notifications,
};
use mirui::ecs::Component;
use mirui::input::event::contract::*;
use mirui::input::event::gesture::Tap;

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

#[mirui::event]
fn emit_hit(node: entity!(), commands: commands!()) {
    commands.emit(node, Hit { strength: 7 });
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
    prepare_notifications(
        &mut world,
        NotificationCapacity {
            records: 1,
            payload_bytes: 1,
            payload_alignment: 1,
        },
    )
    .unwrap();
    type Apply = fn(&mut mirui::ecs::World, mirui::ecs::Entity, Hit) -> Result<(), CommandError>;
    let sender = emit_hit::callback::<Tap, NodeContext>()
        .prepare(
            &mut world,
            CommandCapacity {
                records: 1,
                payload_bytes: std::mem::size_of::<(Hit, Apply)>(),
                payload_alignment: std::mem::align_of::<(Hit, Apply)>(),
            },
        )
        .unwrap();
    let input = Tap {
        x: mirui::types::Fixed::ZERO,
        y: mirui::types::Fixed::ZERO,
        target: node,
    };
    sender.invoke(&mut world, &input, node).unwrap();
    flush_notifications(&mut world, &listeners, 1)
        .unwrap()
        .unwrap();
    assert_eq!(world.resource::<Hits>().unwrap().0, 14);
}
