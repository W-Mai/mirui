use mirui::ecs::World;
use mirui::ui;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View (
                width: @(width, height, id(a).width, id(b).width) { width },
                height: @id(c).height { c.height },
            )
        }
    };
}
