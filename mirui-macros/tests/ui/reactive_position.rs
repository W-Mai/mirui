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

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        View(
            left: ${ left.get() },
            top: ${ top.get() },
        )
    };
}
