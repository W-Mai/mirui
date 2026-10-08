use mirui::app::{App, EventCapacity};
use mirui::ui;
use mirui::ui::widgets::{Slider, slider::ValueChanged};

type Level = Slider;

fn main() {
    let mut app = App::headless(32, 32);
    app.with_event_capacity(
        EventCapacity::new()
            .notifications::<ValueChanged>(1)
            .flush_limit(1),
    );
    let root = app.spawn_root().id();

    let target = app.compose(root, |cx| {
        ui! {
            View () [
                Level::default(),
            ] on Level::ValueChanged { let _ = (event.old, event.new); }
        }
    });
    assert!(app.world.has::<Slider>(target));
}
