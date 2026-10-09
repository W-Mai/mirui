use mirui::__macro_support::{AppContext, EntityContext, ResMut, SystemAccess, event_callback};
use mirui::component;
use mirui::ecs::{CommandCapacity, World};
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};
use mirui::input::event::lifecycle::Started;

#[component]
struct Pad;

struct Hit(String);
impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

struct Hits(usize);

fn main() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Pad);
    world.insert_resource(Hits(0));
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_resource::<Hits>()];
    // SAFETY: ACCESS describes the sole resource parameter; no Models or World
    // pointers are captured by the body.
    let hit = unsafe {
        event_callback::<Hit, EntityContext<Pad>, (ResMut<Hits>,), ()>(
            "hit",
            ACCESS,
            |(hits,), payload, _, _scope| hits.0 += payload.0.len(),
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(
        hit.invoke(&mut world, &Hit(String::from("pad")), node),
        Ok(Some(()))
    );
    assert_eq!(world.resource::<Hits>().unwrap().0, 3);
    // SAFETY: the empty access plan matches the empty parameter set.
    let started = unsafe {
        event_callback::<Started, AppContext, (), ()>("started", &[], |(), _, (), _scope| ())
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(started.invoke(&mut world, &Started, ()), Ok(Some(())));
}
