use mirui::__macro_support::{AppContext, EntityContext, event_callback};
use mirui::component;
use mirui::ecs::{CommandCapacity, Entity, World};
use mirui::input::event::gesture::Tap;
use mirui::input::event::input::Wheel;
use mirui::input::event::lifecycle::Started;
use mirui::ui::widgets::slider;

#[component]
struct Pad;

fn main() {
    unsafe {
        let _ =
            event_callback::<Tap, AppContext, (), bool>("app-tap", &[], |(), _, (), _scope| true);
        let _ = event_callback::<Wheel, EntityContext<Pad>, (), bool>(
            "entity-wheel",
            &[],
            |(), _, _: Entity, _scope| true,
        );
        let _ = event_callback::<slider::ValueChanged, EntityContext<Pad>, (), ()>(
            "wrong-widget",
            &[],
            |(), _, _: Entity, _scope| (),
        );
        let _ = event_callback::<Started, AppContext, (), bool>(
            "notification-result",
            &[],
            |(), _, (), _scope| true,
        );
        let callback = event_callback::<Started, AppContext, (), ()>(
            "app-entity",
            &[],
            |(), _, (), _scope| (),
        )
        .prepare(&mut World::new(), CommandCapacity::default())
        .unwrap();
        callback.invoke(
            &mut World::new(),
            &Started,
            Entity {
                id: 0,
                generation: 0,
            },
        );
    }
}
