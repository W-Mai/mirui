//! Source fixtures use local cells by default and synchronized controls in shared mode.

#[cfg(feature = "sync")]
pub use std::sync::Arc as Rc;
#[cfg(not(feature = "sync"))]
pub use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[cfg(feature = "sync")]
#[derive(Debug)]
pub struct Cell<T>(std::sync::Mutex<T>);

#[cfg(feature = "sync")]
impl<T> Cell<T> {
    pub fn new(value: T) -> Self {
        let cell = Self(std::sync::Mutex::new(value));
        // Prepare platform mutex storage before measuring source hot paths.
        drop(cell.0.lock().unwrap());
        cell
    }
    pub fn replace(&self, value: T) -> T {
        core::mem::replace(&mut *self.0.lock().unwrap(), value)
    }
    pub fn set(&self, value: T) {
        drop(self.replace(value));
    }
}
#[cfg(feature = "sync")]
impl<T: Copy> Cell<T> {
    pub fn get(&self) -> T {
        *self.0.lock().unwrap()
    }
}

#[cfg(feature = "sync")]
impl<T: Default> Default for Cell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(feature = "sync")]
#[derive(Debug)]
pub struct RefCell<T>(std::sync::Mutex<T>);

#[cfg(feature = "sync")]
impl<T> RefCell<T> {
    pub fn new(value: T) -> Self {
        let cell = Self(std::sync::Mutex::new(value));
        drop(cell.0.lock().unwrap());
        cell
    }
    pub fn borrow(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
    pub fn borrow_mut(&self) -> std::sync::MutexGuard<'_, T> {
        self.borrow()
    }
    pub fn replace(&self, value: T) -> T {
        core::mem::replace(&mut *self.0.lock().unwrap(), value)
    }
}

#[cfg(feature = "sync")]
impl<T: Default> Default for RefCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
