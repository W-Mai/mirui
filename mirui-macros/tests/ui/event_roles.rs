use mirui::app::{App, EventCapacity};
use mirui::ecs::{CommandCapacity, Component};
use mirui::input::event::gesture::Tap;
use mirui::ui;

struct Marker(u8);

impl Component for Marker {}

mod foreign {
    macro_rules! com {
        () => {
            9_u8
        };
    }
    pub(crate) use com;

    macro_rules! commands {
        () => {};
    }
    pub(crate) use commands;
}

fn main() {
    let mut app = App::headless(32, 32);
    app.with_event_capacity(
        EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(1)),
    );
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            View () [
                Marker(0),
            ] on Tap(|input: event!(Tap), markers: query!(Marker), commands: commands!()| {
                let _ = foreign::com!();
                foreign::commands!();
                let value = markers.get(input.target).map(|marker| marker.0);
                commands.insert(input.target, Marker(value.unwrap_or(0) + 1));
            })
        }
    });
}
