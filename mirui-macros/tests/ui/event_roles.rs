use mirui::ecs::{Component, World};
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
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap {
                let _ = foreign::com!();
                foreign::commands!();
                let value = mirui::com!(target, Marker).map(|marker| marker.0);
                commands!().insert(target, Marker(value.unwrap_or(0) + 1));
            }
        }
    };
    world.insert(target, Marker(0));
}
