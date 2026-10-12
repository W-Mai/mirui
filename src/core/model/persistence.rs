/// Per-root persistence state selected by generated Model declarations.
#[doc(hidden)]
pub trait PersistenceState: sealed::Sealed + Default + 'static {
    fn changed(&self);
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for () {}
}

impl PersistenceState for () {
    fn changed(&self) {}
}

#[cfg(feature = "persistence")]
mod candidate {
    use core::cell::Cell;

    use super::PersistenceState;

    /// A conservative save candidate, independent of observation and effect delivery.
    #[doc(hidden)]
    pub struct Candidate {
        revision: Cell<u64>,
        pending: Cell<bool>,
    }

    impl super::sealed::Sealed for Candidate {}

    impl Default for Candidate {
        fn default() -> Self {
            Self {
                revision: Cell::new(0),
                pending: Cell::new(true),
            }
        }
    }

    impl PersistenceState for Candidate {
        fn changed(&self) {
            self.revision.set(self.revision.get().saturating_add(1));
            self.pending.set(true);
        }
    }

    impl Candidate {
        pub fn pending(&self) -> bool {
            self.pending.get()
        }

        pub fn stamp(&self) -> u64 {
            self.revision.get()
        }

        /// Confirm only the saved snapshot, never a subsequent mutation.
        pub fn confirm(&self, stamp: u64) {
            if stamp != u64::MAX && self.revision.get() == stamp {
                self.pending.set(false);
            }
        }
    }

    const _: () = assert!(core::mem::size_of::<Candidate>() <= 16);

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stale_confirmation_and_saturated_revisions_remain_pending() {
            let state = Candidate::default();
            assert!(state.pending());
            state.confirm(state.stamp());
            assert!(!state.pending());
            let old = state.stamp();
            state.changed();
            state.confirm(old);
            assert!(state.pending());
            state.confirm(state.stamp());
            assert!(!state.pending());
            state.revision.set(u64::MAX - 1);
            state.changed();
            state.confirm(u64::MAX);
            state.changed();
            assert_eq!(state.stamp(), u64::MAX);
            assert!(state.pending());
        }
    }
}

#[cfg(feature = "persistence")]
pub use candidate::Candidate;

#[cfg(all(test, feature = "std", feature = "persistence"))]
mod tests;
