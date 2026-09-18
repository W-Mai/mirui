use mirui::ecs::World;
use mirui::types::Fixed;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Text;
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

        Column (container: true) {
            Text(
                "Title",
                font_size: @(width, id(stage).height) {
                    if width < stage.height { 18_u16 } else { 30_u16 }
                },
                height: @id(stage).width { stage.width },
                min_height: @(height as available_height) { available_height / 2 },
                max_height: @id("stage").height as named_height { named_height },
                max_width: @width as available_width { available_width },
            )
            View (id: "stage", width: Fixed::from_int(320), height: Fixed::from_int(240))
        }
    };
}
