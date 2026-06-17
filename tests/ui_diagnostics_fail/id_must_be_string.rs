use mirui::ecs::World;
use mirui::ui;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let world_ref = &mut world;

    ui! {
        :(
            parent: root
            world: world_ref
        :)
        slot (id: 42) {}
    };
}
