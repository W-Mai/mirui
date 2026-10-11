pub(super) struct PointerSlots<T, const N: usize> {
    native_ids: [T; N],
    occupied: u32,
}

impl<T: Copy + Default + Eq, const N: usize> PointerSlots<T, N> {
    pub(super) fn new() -> Self {
        const { assert!(N <= 32) };
        Self {
            native_ids: [T::default(); N],
            occupied: 0,
        }
    }

    pub(super) fn clear(&mut self) {
        self.occupied = 0;
    }

    pub(super) fn get(&self, native_id: T) -> Option<u8> {
        self.native_ids.iter().enumerate().find_map(|(slot, &id)| {
            (self.occupied & (1 << slot) != 0 && id == native_id).then_some(slot as u8 + 1)
        })
    }

    pub(super) fn insert(&mut self, native_id: T) -> Option<u8> {
        if let Some(id) = self.get(native_id) {
            return Some(id);
        }
        let Some(slot) = (0..N).find(|slot| self.occupied & (1 << slot) == 0) else {
            crate::warn!(target: "mirui::input", "pointer ignored: all {} slots are occupied", N);
            return None;
        };
        self.native_ids[slot] = native_id;
        self.occupied |= 1 << slot;
        Some(slot as u8 + 1)
    }

    pub(super) fn remove(&mut self, native_id: T) -> Option<u8> {
        let id = self.get(native_id)?;
        self.occupied &= !(1 << (id - 1));
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

    #[test]
    fn slots_preserve_live_identities_and_reserve_mouse_zero() {
        let mut slots = PointerSlots::<u64, 2>::new();
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(slots.insert(1), Some(1));
                assert_eq!(slots.insert(256), Some(2));
                assert_eq!(slots.insert(1), Some(1));
                assert_eq!(slots.insert(u64::MAX), None);
                assert_eq!(slots.get(256), Some(2));
                assert_eq!(slots.remove(1), Some(1));
                assert_eq!(slots.get(1), None);
                assert_eq!(slots.remove(1), None);
                assert_eq!(slots.insert(u64::MAX), Some(1));
                assert_eq!(slots.get(256), Some(2));
                slots.clear();
                assert_eq!(slots.get(u64::MAX), None);
                assert_eq!(slots.get(256), None);
            }),
            0
        );
    }
}
