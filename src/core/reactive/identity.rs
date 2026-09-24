use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct SlotId {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Clone, Copy)]
struct SlotState {
    generation: u32,
    live: bool,
}

#[derive(Default)]
pub(super) struct SlotAllocator {
    states: Vec<SlotState>,
    free: Vec<u32>,
}

impl SlotAllocator {
    pub const fn new() -> Self {
        Self {
            states: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn allocate(&mut self) -> SlotId {
        if let Some(slot) = self.free.pop() {
            let state = &mut self.states[slot as usize];
            state.live = true;
            return SlotId {
                slot,
                generation: state.generation,
            };
        }
        let slot = u32::try_from(self.states.len()).expect("reactive slot identity exhausted");
        self.free.reserve(self.states.len() + 1);
        self.states.push(SlotState {
            generation: 0,
            live: true,
        });
        SlotId {
            slot,
            generation: 0,
        }
    }

    pub fn release(&mut self, id: SlotId) -> bool {
        let Some(state) = self.states.get_mut(id.slot as usize) else {
            return false;
        };
        if !state.live || state.generation != id.generation {
            return false;
        }
        state.live = false;
        state.generation = state
            .generation
            .checked_add(1)
            .expect("reactive slot generation exhausted");
        self.free.push(id.slot);
        true
    }

    pub fn is_live(&self, id: SlotId) -> bool {
        self.states
            .get(id.slot as usize)
            .is_some_and(|state| state.live && state.generation == id.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::SlotAllocator;

    #[test]
    fn reused_slots_reject_stale_generation() {
        let mut slots = SlotAllocator::default();
        let first = slots.allocate();
        assert!(slots.release(first));
        let second = slots.allocate();
        assert_eq!(first.slot, second.slot);
        assert_ne!(first.generation, second.generation);
        assert!(!slots.release(first));
        assert!(slots.release(second));
    }
}
