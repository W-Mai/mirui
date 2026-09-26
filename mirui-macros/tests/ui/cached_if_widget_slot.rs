use mirui::core::reactive::Signal;
use mirui::ecs::World;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::{IdMap, ViewRegistry};

ui!(compose Card {
    @@body
});

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut registry = ViewRegistry::default();
    registry.insert(ui!(compose Card));
    world.insert_resource(registry);
    let root = WidgetBuilder::new(&mut world).id();
    let signal = Signal::new(true);
    let condition = signal.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        View () {
            if $condition {
                Card () { @body { View () } }
            } else {
                Card () { @body { View () } }
            }
        }
    };
}
