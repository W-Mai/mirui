use super::{ListState, ModelList, NodeSlot};
use crate::core::model::{CollectionError, Model, OwnedModel};
use alloc::{rc::Rc, vec::Vec};
use core::cell::Cell;

pub(in crate::core::model::list) struct PreparedBinding<M: Model> {
    state: Rc<ListState<M>>,
    nodes: Vec<NodeSlot<M>>,
    order: Vec<usize>,
}

impl<M: Model> PreparedBinding<M> {
    pub(in crate::core::model::list) fn new(
        state: Rc<ListState<M>>,
        capacity: usize,
    ) -> Result<Self, CollectionError> {
        let mut nodes = Vec::new();
        let mut order = Vec::new();
        nodes
            .try_reserve_exact(capacity)
            .map_err(|_| CollectionError::Allocation)?;
        order
            .try_reserve_exact(capacity)
            .map_err(|_| CollectionError::Allocation)?;
        Ok(Self {
            state,
            nodes,
            order,
        })
    }

    pub(in crate::core::model::list) fn prepare(&mut self, value: &mut ModelList<M>) {
        for (index, slot) in value.slots.iter().enumerate() {
            let node = if let Some(position) = slot.position {
                let storage = value.entries[position].value.storage_mut();
                let node = Rc::new((self.state.factory)(index, Some(storage)));
                node.begin(storage);
                node
            } else if index < self.state.nodes.borrow().len() {
                self.state.node(index)
            } else {
                Rc::new((self.state.factory)(index, None))
            };
            self.nodes.push(NodeSlot {
                node,
                generation: Cell::new(slot.generation),
            });
        }
    }

    pub(in crate::core::model::list) fn install(self, value: &mut ModelList<M>) {
        self.state.each_node(|node| node.retire());
        *self.state.nodes.borrow_mut() = self.nodes;
        *self.state.order.borrow_mut() = self.order;
        self.state.revision.set(value.revision);
        self.state.active.set(true);
        value.binding = Some(self.state);
    }
}
