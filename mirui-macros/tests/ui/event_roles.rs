use mirui::app::{App, EventCapacity};
use mirui::ecs::{CommandCapacity, Component};
use mirui::input::event::gesture::Tap;
use mirui::ui;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;

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
    let mut world = app.world;
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap(|input: event!(Tap), markers: query!(Marker), commands: commands!()| {
                let _ = foreign::com!();
                foreign::commands!();
                let value = markers.get(input.target).map(|marker| marker.0);
                commands.insert(input.target, Marker(value.unwrap_or(0) + 1));
            })
        }
    };
    world.insert(target, Marker(0));
}
