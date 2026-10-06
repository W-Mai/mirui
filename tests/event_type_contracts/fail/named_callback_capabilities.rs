use mirui::__macro_support::{AppContext, EntityContext};
use mirui::ecs::{CommandCapacity, World};
use mirui::input::event::input::Key;
use mirui::input::event::lifecycle::Started;
use mirui::ui::widgets::{Checkbox, Slider};
use mirui::ui::widgets::slider;
use mirui::event;

#[event]
fn requires_entity(node: entity!()) {
    let _ = node;
}

#[event]
fn started() -> bool {
    true
}

#[event]
fn slider_changed(input: event!(slider::ValueChanged)) {
    let _ = input;
}

#[event]
fn needs_key(input: event!(Key)) {
    let _ = input;
}

fn main() {
    let mut world = World::new();
    let _ = requires_entity::callback::<Key, AppContext>();
    let _ = started::callback::<Started, AppContext>();
    let _ = slider_changed::callback::<EntityContext<Checkbox>>();
    let needs_key = needs_key::callback::<AppContext>()
        .prepare(&mut world, CommandCapacity::default()).unwrap();
    let _ = needs_key.invoke(&mut world, &Started, ());
    let _ = slider_changed::callback::<EntityContext<Slider>>();
}
