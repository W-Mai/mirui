use mirui::ecs::{Entity, World};
use mirui::{component, view};

#[component]
struct Tile;

#[view(component = Tile)]
fn tile() {}

fn attach(_: &mut World, _: Entity) {}

fn observe(_: &World, _: Entity, _: &mut mirui::__macro_support::ViewObservationBindings) {}

fn main() {
    let _ = tile::view().with_attach(attach);
    let _ = tile::view().with_filter::<Tile>();
    let _ = tile::view().with_observation(observe, 0);
    let _ = tile::view().with_query_dependencies(&[]);
}
