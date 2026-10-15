use core::cell::Cell;

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

    pub(crate) fn needs_redraw(&self) -> bool {
        self.invalid.get()
    }

    pub(crate) fn acknowledge(&self) {
        self.invalid.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::BackbufferInvalidation;
    use crate::types::Fixed;

    #[test]
    fn initial_and_changed_backing_stores_require_one_full_frame() {
        let state = BackbufferInvalidation::new((480, 320, Fixed::ONE));
        assert!(state.needs_redraw());
        assert!(state.needs_redraw());
        state.acknowledge();
        assert!(!state.needs_redraw());

        state.observe((480, 320, Fixed::ONE), false);
        assert!(!state.needs_redraw());

        state.observe((640, 320, Fixed::ONE), false);
        assert!(state.needs_redraw());
        assert!(state.needs_redraw());
        state.acknowledge();
        assert!(!state.needs_redraw());

        state.observe((640, 320, Fixed::from_int(2)), false);
        assert!(state.needs_redraw());

        state.observe((640, 320, Fixed::from_int(2)), true);
        assert!(state.needs_redraw());
        state.acknowledge();
        assert!(!state.needs_redraw());
    }
}
