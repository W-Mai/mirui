use mirui::ecs::World;
use mirui::ui;
use mirui::widget::IdMap;
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        View () {
            on Tap { todo!() }
        }
    };
}
