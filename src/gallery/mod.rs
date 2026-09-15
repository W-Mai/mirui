use alloc::vec::Vec;
use core::cell::RefCell;

pub mod demos;

pub(crate) struct SceneRgbaScratch(RefCell<Vec<u8>>);

impl SceneRgbaScratch {
    #[cfg(feature = "std")]
    pub(crate) fn new(bytes: usize) -> Self {
        Self(RefCell::new(alloc::vec![0; bytes]))
    }

    #[cfg(feature = "std")]
    pub(crate) fn ensure_capacity(&mut self, bytes: usize) {
        if self.0.get_mut().len() < bytes {
            self.0.get_mut().resize(bytes, 0);
        }
    }

    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        f(self.0.borrow_mut().as_mut_slice())
    }

    #[cfg(feature = "std")]
    pub(crate) fn install(world: &mut crate::ecs::World, bytes: usize) {
        if let Some(scratch) = world.resource_mut::<Self>() {
            scratch.ensure_capacity(bytes);
        } else {
            world.insert_resource(Self::new(bytes));
        }
    }
}
