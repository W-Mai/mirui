#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SlotSelection {
    Hit(usize),
    Rewrite(usize),
    Free(usize),
    Replace(usize),
}

pub(super) fn select_slot<I: Copy + Eq, K: Copy + Eq>(
    slots: impl IntoIterator<Item = (Option<I>, Option<K>, u64)>,
    identity: I,
    key: K,
) -> Option<SlotSelection> {
    let mut free = None;
    let mut lru = None;
    for (index, (owner, retained_key, last_used)) in slots.into_iter().enumerate() {
        if owner == Some(identity) {
            return Some(if retained_key == Some(key) {
                SlotSelection::Hit(index)
            } else {
                SlotSelection::Rewrite(index)
            });
        }
        if owner.is_none() {
            free.get_or_insert(index);
        } else if lru.is_none_or(|(_, used)| last_used < used) {
            lru = Some((index, last_used));
        }
    }
    free.map(SlotSelection::Free)
        .or_else(|| lru.map(|(index, _)| SlotSelection::Replace(index)))
}

#[cfg(test)]
mod tests {
    use super::{SlotSelection, select_slot};

    #[test]
    fn changed_content_rewrites_the_same_run_without_displacing_others() {
        let slots = [(Some(1_u8), Some(10_u8), 1), (Some(2), Some(20), 2)];
        assert_eq!(select_slot(slots, 1, 11), Some(SlotSelection::Rewrite(0)));
        assert_eq!(select_slot(slots, 2, 20), Some(SlotSelection::Hit(1)));
    }

    #[test]
    fn identical_content_from_another_run_does_not_claim_its_slot() {
        let slots = [(Some(1_u8), Some(10_u8), 1), (None, None, 0)];
        assert_eq!(select_slot(slots, 2, 10), Some(SlotSelection::Free(1)));
    }

    #[test]
    fn new_run_uses_a_free_slot_before_the_lru_slot() {
        let slots = [
            (Some(1_u8), Some(10_u8), 1),
            (None, None, 0),
            (Some(2), Some(20), 2),
        ];
        assert_eq!(select_slot(slots, 3, 30), Some(SlotSelection::Free(1)));
    }

    #[test]
    fn new_run_replaces_the_least_recently_used_owner_when_full() {
        let slots = [
            (Some(1_u8), Some(10_u8), 8),
            (Some(2), Some(20), 3),
            (Some(3), None, 5),
        ];
        assert_eq!(select_slot(slots, 4, 40), Some(SlotSelection::Replace(1)));
        assert_eq!(select_slot(slots, 3, 30), Some(SlotSelection::Rewrite(2)));
    }
}
