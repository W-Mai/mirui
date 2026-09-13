use core::cell::Cell;

use super::BackbufferPersistence;
use crate::types::Fixed;

pub(crate) struct BackbufferInvalidation {
    display: Cell<(u16, u16, Fixed)>,
    invalid: Cell<bool>,
}

impl BackbufferInvalidation {
    pub(crate) const fn new(display: (u16, u16, Fixed)) -> Self {
        Self {
            display: Cell::new(display),
            invalid: Cell::new(true),
        }
    }

    pub(crate) fn observe(&self, display: (u16, u16, Fixed), backing_reset: bool) {
        if self.display.replace(display) != display || backing_reset {
            self.invalid.set(true);
        }
    }

    pub(crate) fn take_persistence(&self) -> BackbufferPersistence {
        if self.invalid.replace(false) {
            BackbufferPersistence::Transient
        } else {
            BackbufferPersistence::Persistent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BackbufferInvalidation;
    use crate::surface::BackbufferPersistence;
    use crate::types::Fixed;

    #[test]
    fn initial_and_changed_backing_stores_require_one_full_frame() {
        let state = BackbufferInvalidation::new((480, 320, Fixed::ONE));
        assert_eq!(state.take_persistence(), BackbufferPersistence::Transient);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Persistent);

        state.observe((480, 320, Fixed::ONE), false);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Persistent);

        state.observe((640, 320, Fixed::ONE), false);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Transient);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Persistent);

        state.observe((640, 320, Fixed::from_int(2)), false);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Transient);

        state.observe((640, 320, Fixed::from_int(2)), true);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Transient);
        assert_eq!(state.take_persistence(), BackbufferPersistence::Persistent);
    }
}
