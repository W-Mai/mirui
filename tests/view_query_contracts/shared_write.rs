use mirui::__macro_support::{ComponentDriver, QueryParam, QueryReadScope, Write};
use mirui::component;
use mirui::ecs::World;

#[component]
struct Tile;

fn main() {
    let mut world = World::new();
    let scope = QueryReadScope::new(world.spawn_empty());
    let _ = QueryParam::<ComponentDriver<Tile>, Write<Tile>, ()>::read_for_view(&world, &scope);
}
