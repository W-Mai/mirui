use mirui::core::reactive::Signal;
use mirui::ecs::World;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    let root = WidgetBuilder::new(&mut world).id();
    let signal = Signal::new(true);
    let condition = signal.clone();
    let inner = signal.clone();

    {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () {
                if $condition {
                    if $inner {
                        View ()
                    }
                }
            }
        }
    };
}
