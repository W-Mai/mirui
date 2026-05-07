use alloc::vec::Vec;

use super::World;

pub type System = fn(&mut World);

#[derive(Default)]
pub struct SystemScheduler {
    systems: Vec<System>,
}

impl SystemScheduler {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    pub fn add(&mut self, system: System) {
        self.systems.push(system);
    }

    pub fn run_all(&self, world: &mut World) {
        for system in &self.systems {
            system(world);
        }
    }
}
