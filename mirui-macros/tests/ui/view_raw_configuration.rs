use mirui::ecs::{CommandError, Entity, World};
use mirui::input::event::gesture::GestureEvent;
use mirui::{component, view};

#[component]
struct Tile;

#[view(component = Tile)]
fn tile() {}

fn attach(_: &mut World, _: Entity) {}

fn gesture(_: &mut World, _: Entity, _: &GestureEvent) -> Result<bool, CommandError> {
    Ok(false)
}

fn observe(_: &World, _: Entity, _: &mut mirui::__macro_support::ViewObservationBindings) {}

fn main() {
    let _ = tile::view().with_attach(attach);
    let _ = tile::view().with_internal_gesture(gesture);
    let _ = tile::view().with_filter::<Tile>();
    let _ = tile::view().with_observation(observe, 0);
    let _ = tile::view().with_query_dependencies(&[]);
}
