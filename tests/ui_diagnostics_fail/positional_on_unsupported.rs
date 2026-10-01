use mirui::ecs::World;
use mirui::ui;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Button;

fn main() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let world_ref = &mut world;

    {
        let mut cx = ::mirui::ui::UiScope::new(world_ref, root);
        ui! {
            Button (42)
        }
    };

    let _ = Button::default();
}
