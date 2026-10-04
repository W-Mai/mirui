use mirui::__macro_support::{AppContext, DeferredCommands, deferred_event_callback};
use mirui::input::event::input::Key;
use std::cell::Cell;

fn main() {
    let retained: Cell<Option<DeferredCommands<'static>>> = Cell::new(None);
    unsafe {
        let _ = deferred_event_callback::<Key, AppContext, (), ()>(
            "escape-commands",
            &[],
            move |(), _, (), commands| retained.set(Some(commands)),
        );
    }
}
