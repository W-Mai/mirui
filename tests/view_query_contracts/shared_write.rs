use mirui::__macro_support::{ComponentDriver, QueryParam, Write};
use mirui::component;
use mirui::ecs::World;

#[component]
struct Tile;

fn main() {
    let world = World::new();
    let _ = QueryParam::<ComponentDriver<Tile>, Write<Tile>, ()>::read_shared(&world);
}
