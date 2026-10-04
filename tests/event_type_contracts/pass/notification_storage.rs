use mirui::__macro_support::{NotificationCapacity, NotificationQueue};
use mirui::component;
use mirui::ecs::Entity;
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};

#[component]
struct Pad;

#[derive(Debug)]
struct Hit(String);

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

fn main() {
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 2,
            payload_bytes: 2 * core::mem::size_of::<Hit>(),
            payload_alignment: core::mem::align_of::<Hit>(),
        })
        .unwrap();
    let entity = Entity {
        id: 1,
        generation: 2,
    };
    queue
        .push_entity(entity, Hit(String::from("first")))
        .unwrap();
    queue
        .push_entity(entity, Hit(String::from("second")))
        .unwrap();
    let mut seen = Vec::new();
    queue
        .flush(2, |notification| {
            assert_eq!(notification.entity(), Some(entity));
            seen.push(notification.payload::<Hit>().unwrap().0.clone());
        })
        .unwrap();
    assert_eq!(seen, ["first", "second"]);
}
