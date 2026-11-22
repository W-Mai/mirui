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
#[path = "../../../tests/unit/render/backends/web_glyph_slot.rs"]
mod tests;
