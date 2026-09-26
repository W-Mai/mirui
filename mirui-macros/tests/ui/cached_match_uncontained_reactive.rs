use mirui::core::reactive::Signal;
use mirui::ecs::World;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    let root = WidgetBuilder::new(&mut world).id();
    let signal = Signal::new(true);
    let selection = signal.clone();
    let inner = signal.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        View () {
            match $selection {
                true => {
                    if $inner {
                        View ()
                    }
                }
                false => {}
            }
        }
    };
}
