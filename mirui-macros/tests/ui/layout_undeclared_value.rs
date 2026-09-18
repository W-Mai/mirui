use mirui::ecs::World;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui;

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)
        View (width: @width { width.min(height) })
    };
}
