use mirui::__macro_support::{AppContext, ResMut, SystemAccess, deferred_event_callback};
use mirui::ecs::{CommandCapacity, World};
use mirui::input::event::BubbleControl;
use mirui::input::event::input::Key;

struct State(u32);

fn main() {
    let mut world = World::new();
    world.insert_resource(State(1));
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_resource::<State>()];
    // SAFETY: ACCESS matches the sole parameter. Resource removal is recorded,
    // not performed while the parameter reference lives.
    let callback = unsafe {
        deferred_event_callback::<Key, AppContext, (ResMut<State>,), bool>(
            "remove-resource",
            ACCESS,
            |(mut state,), _, (), _scope, mut commands| {
                commands.remove_resource::<State>();
                state.0 += 1;
                true
            },
        )
    }
    .prepare(
        &mut world,
        CommandCapacity::new().remove_resource::<State>(1),
    )
    .unwrap();
    let payload = Key {
        code: 1,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Ok(Some(BubbleControl::Prevent))
    );
    assert!(world.resource::<State>().is_none());
}
