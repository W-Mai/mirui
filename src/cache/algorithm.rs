use super::lookup::NodeId;

pub trait Algorithm {
    type State: Default;

    fn on_access(state: &mut Self::State, node_id: NodeId);
    fn on_insert(state: &mut Self::State, node_id: NodeId);
    fn on_remove(state: &mut Self::State, node_id: NodeId);
    fn pick_victim(state: &Self::State) -> Option<NodeId>;
}

#[derive(Default)]
pub struct Lru;

#[derive(Default)]
pub struct LruState {
    list: IntrusiveList,
}

impl Algorithm for Lru {
    type State = LruState;

    fn on_access(state: &mut LruState, node_id: NodeId) {
        state.list.move_to_front(node_id);
    }

    fn on_insert(state: &mut LruState, node_id: NodeId) {
        state.list.push_front(node_id);
    }

    fn on_remove(state: &mut LruState, node_id: NodeId) {
        state.list.remove(node_id);
    }

    fn pick_victim(state: &LruState) -> Option<NodeId> {
        state.list.back()
    }
}

// `slots` indexed by NodeId; never shrunk. Caller reuses ids via the
// cache arena, so slots tracks the id high-water mark.
#[derive(Default)]
struct IntrusiveList {
    slots: alloc::vec::Vec<Option<LinkSlot>>,
    head: Option<NodeId>,
    tail: Option<NodeId>,
}

#[derive(Clone, Copy)]
struct LinkSlot {
    prev: Option<NodeId>,
    next: Option<NodeId>,
}

impl IntrusiveList {
    fn ensure_slot(&mut self, node_id: NodeId) {
        if node_id >= self.slots.len() {
            self.slots.resize(node_id + 1, None);
        }
    }

    fn push_front(&mut self, node_id: NodeId) {
        self.ensure_slot(node_id);
        let old_head = self.head;
        self.slots[node_id] = Some(LinkSlot {
            prev: None,
            next: old_head,
        });
        if let Some(h) = old_head {
            if let Some(slot) = self.slots[h].as_mut() {
                slot.prev = Some(node_id);
            }
        } else {
            self.tail = Some(node_id);
        }
        self.head = Some(node_id);
    }

    fn remove(&mut self, node_id: NodeId) {
        if node_id >= self.slots.len() {
            return;
        }
        let Some(slot) = self.slots[node_id].take() else {
            return;
        };
        match slot.prev {
            Some(p) => {
                if let Some(p_slot) = self.slots[p].as_mut() {
                    p_slot.next = slot.next;
                }
            }
            None => self.head = slot.next,
        }
        match slot.next {
            Some(n) => {
                if let Some(n_slot) = self.slots[n].as_mut() {
                    n_slot.prev = slot.prev;
                }
            }
            None => self.tail = slot.prev,
        }
    }

    fn move_to_front(&mut self, node_id: NodeId) {
        if self.head == Some(node_id) {
            return;
        }
        self.remove(node_id);
        self.push_front(node_id);
    }

    fn back(&self) -> Option<NodeId> {
        self.tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_picks_oldest() {
        let mut s = LruState::default();
        Lru::on_insert(&mut s, 1);
        Lru::on_insert(&mut s, 2);
        Lru::on_insert(&mut s, 3);
        // List front->back: 3, 2, 1
        assert_eq!(Lru::pick_victim(&s), Some(1));

        // Touch 1 → moves to front
        Lru::on_access(&mut s, 1);
        assert_eq!(Lru::pick_victim(&s), Some(2));
    }

    #[test]
    fn lru_remove_middle() {
        let mut s = LruState::default();
        Lru::on_insert(&mut s, 1);
        Lru::on_insert(&mut s, 2);
        Lru::on_insert(&mut s, 3);
        Lru::on_remove(&mut s, 2);
        assert_eq!(Lru::pick_victim(&s), Some(1));
        Lru::on_remove(&mut s, 1);
        assert_eq!(Lru::pick_victim(&s), Some(3));
        Lru::on_remove(&mut s, 3);
        assert_eq!(Lru::pick_victim(&s), None);
    }
}
