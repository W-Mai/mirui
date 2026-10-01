use mirui::core::reactive::Signal;
use mirui::ecs::World;
use mirui::types::Dimension;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;

fn main() {
    let mut world = World::new();
    let root = WidgetBuilder::new(&mut world).id();
    let left = Signal::new(12_i32);
    let top = Signal::new(Dimension::percent(25));

    {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View (
                left: ${ left.get() },
                top: ${ top.get() }
            )
        }
    };
}
