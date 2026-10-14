use mirui::__macro_support::{AppContext, Res, SystemAccess, event_callback};
use mirui::input::event::input::Key;
use std::cell::Cell;

struct State(u32);

fn main() {
    const ACCESS: &[SystemAccess] = &[SystemAccess::read_resource::<State>()];
    let retained: Cell<Option<&'static State>> = Cell::new(None);
    unsafe {
        let _ = event_callback::<Key, AppContext, (Res<State>,), ()>(
            "escape-resource",
            ACCESS,
            move |(state,), _, (), _scope| retained.set(Some(&*state)),
        );
        let _ = event_callback::<Key, AppContext, (), ()>(
            "mutable-payload",
            &[],
            |(), event, (), _scope| event.pressed = false,
        );
    }
}
