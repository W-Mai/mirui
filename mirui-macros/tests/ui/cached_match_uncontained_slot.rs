use mirui::core::reactive::Signal;
use mirui::ecs::World;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;

ui!(compose Card {
    @@body
});

fn main() {
    let mut world = World::new();
    let root = WidgetBuilder::new(&mut world).id();
    let signal = Signal::new(true);
    let selection = signal.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Card () {
            match $selection {
                true => {
                    @body { View () }
                }
                false => {
                    @body { View () }
                }
            }
        }
    };
}
