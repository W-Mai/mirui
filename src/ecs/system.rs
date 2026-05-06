use super::world::World;

pub trait System {
    fn run(&self, world: &mut World);
}
