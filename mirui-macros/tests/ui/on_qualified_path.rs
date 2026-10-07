use mirui::app::{App, EventCapacity};
use mirui::ui;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::{Slider, slider::ValueChanged};

type Level = Slider;

fn main() {
    let mut app = App::headless(32, 32);
    app.with_event_capacity(
        EventCapacity::new()
            .notifications::<ValueChanged>(1)
            .flush_limit(1),
    );
    let mut world = app.world;
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();

    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () [
                Level::default(),
            ] on Level::ValueChanged { let _ = (event.old, event.new); }
        }
    };
    assert!(world.has::<Slider>(target));
}
