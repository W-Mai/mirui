use alloc::boxed::Box;
use alloc::vec::Vec;

use super::World;

pub type System = fn(&mut World);

type BoxSystem = Box<dyn FnMut(&mut World)>;

#[derive(Default)]
pub struct SystemScheduler {
    systems: Vec<BoxSystem>,
}

impl SystemScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, system: System) {
        self.systems.push(Box::new(system));
    }

    pub fn add_fn(&mut self, system: impl FnMut(&mut World) + 'static) {
        self.systems.push(Box::new(system));
    }

    pub fn run_all(&mut self, world: &mut World) {
        for system in &mut self.systems {
            system(world);
        }
    }
}
