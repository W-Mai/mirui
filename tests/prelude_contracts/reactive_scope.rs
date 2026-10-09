use mirui::ecs::World;

fn main() {
    let mut world = World::new();
    mirui::core::reactive::with_world_scope(&mut world, || {});
}
