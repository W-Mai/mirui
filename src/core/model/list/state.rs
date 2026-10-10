use super::{ModelList, next_collection_id};
use crate::core::model::{
    BindingOwner, ChildNode, Model, ModelChildren, ModelMetadata, ModelPath, ModelRoot,
    ModelUpdate, OwnedModel, OwnedStorage,
};
use alloc::{
    boxed::Box,
    rc::{Rc, Weak},
    vec::Vec,
};
use core::cell::{Cell, RefCell};

type NodeFactory<M> = Box<dyn Fn(usize, Option<&mut OwnedStorage<M>>) -> ChildNode<M>>;

struct NodeSlot<M: Model> {
    node: Rc<ChildNode<M>>,
    generation: Cell<u64>,
}

pub(super) struct ListState<M: Model> {
    pub(super) owner: BindingOwner,
    identity: u64,
    pub(super) revision: Cell<u64>,
    pub(super) active: Cell<bool>,
    pub(super) leases: Cell<usize>,
    nodes: RefCell<Vec<NodeSlot<M>>>,
    order: RefCell<Vec<usize>>,
    factory: NodeFactory<M>,
}

impl<M: Model> ListState<M> {
    pub(super) fn node(&self, index: usize) -> Rc<ChildNode<M>> {
        self.nodes.borrow()[index].node.clone()
    }

    pub(super) fn set_generation(&self, index: usize, generation: u64) {
        self.nodes.borrow()[index].generation.set(generation);
    }

    pub(super) fn generation(&self, index: usize) -> Option<u64> {
        self.nodes
            .borrow()
            .get(index)
            .map(|slot| slot.generation.get())
    }

    pub(super) fn activate_loaded(&self, index: usize, storage: &mut OwnedStorage<M>) {
        let node = self.node(index);
        if node.can_activate(storage, self.owner) {
            assert!(node.activate(storage, self.owner));
        } else {
            let node = Rc::new((self.factory)(index, Some(storage)));
            self.nodes.borrow_mut()[index].node = node.clone();
            node.begin(storage);
        }
    }

    pub(super) fn prepare(&self, value: &mut ModelList<M>) {
        let capacity = value.capacity();
        {
            let mut nodes = self.nodes.borrow_mut();
            let additional = capacity.saturating_sub(nodes.len());
            nodes.reserve(additional);
        }
        {
            let mut order = self.order.borrow_mut();
            let additional = capacity.saturating_sub(order.len());
            order.reserve(additional);
        }
        while self.nodes.borrow().len() < capacity {
            let index = self.nodes.borrow().len();
            let storage = value.slots[index]
                .position
                .map(|position| value.entries[position].value.storage_mut());
            let node = (self.factory)(index, storage);
            self.nodes.borrow_mut().push(NodeSlot {
                node: Rc::new(node),
                generation: Cell::new(value.slots[index].generation),
            });
        }
        for (index, slot) in value.slots.iter_mut().enumerate() {
            if slot.position.is_none() {
                slot.generation = slot
                    .generation
                    .max(self.nodes.borrow()[index].generation.get());
            }
        }
    }

    fn accepts(&self, value: &ModelList<M>) -> bool {
        self.nodes.borrow().len() >= value.capacity()
            && value.entries.iter().all(|entry| {
                self.nodes.borrow()[entry.slot].generation.get() < u64::MAX
                    && self
                        .node(entry.slot)
                        .can_rebind(&entry.value.storage().value, self.owner)
            })
    }

    fn can_activate(&self, value: &ModelList<M>, owner: BindingOwner) -> bool {
        self.owner.merge(owner).check(self.world()).is_ok()
            && self.accepts(value)
            && value.entries.iter().all(|entry| {
                self.nodes.borrow()[entry.slot].generation.get() < u64::MAX
                    && self
                        .node(entry.slot)
                        .can_activate(entry.value.storage(), owner)
            })
    }

    fn world(&self) -> crate::ecs::world::WorldId {
        match self.owner.0 {
            crate::core::model::OwnerState::Bound(world) => world,
            _ => unreachable!("registered list owner"),
        }
    }

    fn bind_prepared(self: &Rc<Self>, value: &mut ModelList<M>) {
        value.identity = self.identity;
        value.revision = self.revision.get();
        value.binding = Some(self.clone());
        self.active.set(true);
        for (index, slot) in value.slots.iter_mut().enumerate() {
            slot.generation = self.nodes.borrow()[index].generation.get();
        }
        for entry in &mut value.entries {
            let generation = value.slots[entry.slot]
                .generation
                .checked_add(1)
                .expect("model item identity exhausted");
            value.slots[entry.slot].generation = generation;
            self.set_generation(entry.slot, generation);
            self.node(entry.slot)
                .activate_prepared(entry.value.storage_mut(), self.owner);
        }
    }

    pub(super) fn ensure_binding(self: &Rc<Self>, value: &mut ModelList<M>) {
        if value
            .binding
            .as_ref()
            .is_some_and(|bound| Rc::ptr_eq(bound, self))
        {
            return;
        }
        let old_len = self.nodes.borrow().len();
        for index in 0..old_len {
            self.node(index).retire();
        }
        self.prepare(value);
        value.identity = self.identity;
        value.revision = self
            .revision
            .get()
            .checked_add(1)
            .expect("model list revision exhausted");
        self.revision.set(value.revision);
        value.binding = Some(self.clone());
        self.active.set(true);
        for (index, slot) in value.slots.iter_mut().enumerate() {
            slot.generation = self.nodes.borrow()[index].generation.get();
        }
        for entry in &mut value.entries {
            if entry.slot < old_len {
                let generation = value.slots[entry.slot]
                    .generation
                    .checked_add(1)
                    .expect("model item identity exhausted");
                value.slots[entry.slot].generation = generation;
                self.set_generation(entry.slot, generation);
                self.activate_loaded(entry.slot, entry.value.storage_mut());
            } else {
                self.node(entry.slot).begin(entry.value.storage());
            }
        }
    }

    fn each_node(&self, mut visit: impl FnMut(&ChildNode<M>)) {
        let len = self.nodes.borrow().len();
        for index in 0..len {
            visit(&self.node(index));
        }
    }

    fn ordered(&self, mut visit: impl FnMut(&ChildNode<M>)) {
        let len = self.order.borrow().len();
        for index in 0..len {
            let slot = self.order.borrow()[index];
            visit(&self.node(slot));
        }
    }
}

/// Prepared child storage for a model list.
#[doc(hidden)]
pub struct ListChildren<M: Model>(pub(super) Rc<ListState<M>>);

impl<M: Model> ModelChildren<ModelList<M>> for ListChildren<M> {
    fn register<R: Model, P: ModelPath<R, Target = ModelList<M>>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
        value: &mut Option<&mut ModelList<M>>,
    ) -> Self {
        let root = root.clone();
        let state = Rc::new(ListState {
            owner,
            identity: value
                .as_ref()
                .map_or_else(next_collection_id, |value| value.identity),
            revision: Cell::new(value.as_ref().map_or(0, |value| value.revision)),
            active: Cell::new(value.is_some()),
            leases: Cell::new(0),
            nodes: RefCell::new(Vec::new()),
            order: RefCell::new(Vec::new()),
            factory: Box::new(move |index, storage| {
                ChildNode::new::<R, SlotPath<P>>(
                    &root,
                    owner,
                    SlotPath {
                        parent: path,
                        index,
                    },
                    storage,
                )
            }),
        });
        if let Some(value) = value.as_deref_mut() {
            state.prepare(value);
            value.binding = Some(state.clone());
        }
        Self(state)
    }

    fn begin(&self, value: &ModelList<M>) {
        for entry in &value.entries {
            self.0.node(entry.slot).begin(entry.value.storage());
        }
    }
    fn is_writable(&self) -> bool {
        self.0.leases.get() == 0
            && (0..self.0.nodes.borrow().len()).all(|index| self.0.node(index).is_writable())
    }
    fn accepts(&self, value: &ModelList<M>) -> bool {
        self.0.accepts(value)
    }
    fn can_rebind(&self, value: &ModelList<M>, _: BindingOwner) -> bool {
        self.0.accepts(value)
    }
    fn can_activate(&self, value: &ModelList<M>, owner: BindingOwner) -> bool {
        self.0.can_activate(value, owner)
    }
    fn activate(&self, value: &mut ModelList<M>, _: BindingOwner) {
        self.0.bind_prepared(value);
    }
    fn extract(&self, value: &mut ModelList<M>) {
        self.0.ensure_binding(value);
        for entry in &mut value.entries {
            self.0.node(entry.slot).extract(entry.value.storage_mut());
        }
    }
    fn finish(&self, value: &mut ModelList<M>, reset: bool) {
        for entry in &mut value.entries {
            self.0
                .node(entry.slot)
                .finish(entry.value.storage_mut(), reset);
        }
    }
    fn capture_after(&self, value: &ModelList<M>) {
        self.0.order.borrow_mut().clear();
        for entry in &value.entries {
            self.0.node(entry.slot).capture_after(entry.value.storage());
            self.0.order.borrow_mut().push(entry.slot);
        }
    }
    fn publish(&self) {
        self.0.ordered(|node| node.publish());
    }
    fn deliver(&self) {
        self.0.ordered(|node| node.deliver());
    }
    fn discard(&self) {
        self.0.each_node(|node| node.discard());
        self.0.order.borrow_mut().clear();
    }
    fn retire(&self) {
        self.0.active.set(false);
        self.0.each_node(|node| node.retire());
    }
}

#[derive(Clone, Copy)]
struct SlotPath<P> {
    parent: P,
    index: usize,
}

impl<R: Model, M: Model, P: ModelPath<R, Target = ModelList<M>>> ModelPath<R> for SlotPath<P> {
    type Target = M;
    fn read<'a>(&self, root: &'a R) -> &'a M {
        let list = self.parent.read(root);
        &list.entries[list.slots[self.index].position.expect("active model slot")]
            .value
            .storage()
            .value
    }
    fn write<'a>(&self, root: &'a mut R) -> &'a mut M {
        let list = self.parent.write(root);
        &mut list.entries[list.slots[self.index].position.expect("active model slot")]
            .value
            .storage_mut()
            .value
    }
    fn metadata<O>(&self, root: &ModelMetadata<R>, read: impl FnOnce(&ModelMetadata<M>) -> O) -> O {
        self.parent.metadata(root, |metadata| {
            read(metadata.children().0.node(self.index).metadata())
        })
    }
    fn update(
        &self,
        root: &mut R,
        _: &mut R::PendingWatches,
        update: &mut ModelUpdate<'_, M>,
    ) -> bool {
        let list = self.parent.write(root);
        let storage = list.entries[list.slots[self.index].position.expect("active model slot")]
            .value
            .storage_mut();
        let Some(change) = update(&mut storage.value) else {
            return false;
        };
        M::record_change(&mut storage.pending, change);
        true
    }
}
