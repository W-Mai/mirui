//! Bounded dependency storage for the ambient reactive runtime.
//!
//! A shared source has one identity and one value. Each owner partition keeps
//! only a source proxy, so an owner can subscribe without copying that value.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::ecs::Entity;
use crate::ecs::world::WorldId;

use super::Subscriber;
use super::identity::SlotId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PartitionKey {
    Shared,
    Owner { world: WorldId, epoch: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SourceKey {
    pub partition: PartitionKey,
    pub slot: SlotId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConsumerKey {
    pub partition: PartitionKey,
    pub slot: SlotId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CapacityKind {
    Owners,
    Sources,
    Consumers,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingKind {
    Any,
    NonComputed,
    Computed,
}

impl PendingKind {
    fn matches(self, target: Subscriber) -> bool {
        match self {
            Self::Any => true,
            Self::NonComputed => !matches!(target, Subscriber::Computed(_)),
            Self::Computed => matches!(target, Subscriber::Computed(_)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GraphError {
    Full {
        partition: PartitionKey,
        kind: CapacityKind,
        limit: usize,
    },
    MissingOwner(WorldId),
    DuplicateOwner(WorldId),
    StaleSource,
    StaleConsumer,
    CrossOwner {
        source: PartitionKey,
        consumer: PartitionKey,
    },
    UnownedWidget,
    CapacityOverflow,
    IdentityExhausted,
    Allocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GraphCapacity {
    pub shared_sources: usize,
    pub shared_consumers: usize,
    pub owner_sources: usize,
    pub owner_consumers: usize,
    pub owners: usize,
}

impl GraphCapacity {
    pub const fn new(
        shared_sources: usize,
        shared_consumers: usize,
        owner_sources: usize,
        owner_consumers: usize,
        owners: usize,
    ) -> Self {
        Self {
            shared_sources,
            shared_consumers,
            owner_sources,
            owner_consumers,
            owners,
        }
    }
}

#[derive(Clone, Copy)]
struct SourceNode {
    generation: u32,
    live: bool,
}

#[derive(Clone, Copy)]
struct ConsumerNode {
    generation: u32,
    live: bool,
    pending: bool,
    target: Subscriber,
    automatic: bool,
}

struct BitGraph {
    sources: Vec<SourceNode>,
    consumers: Vec<ConsumerNode>,
    rows: Vec<u64>,
    pending: VecDeque<SlotId>,
    source_limit: usize,
    consumer_limit: usize,
    words_per_row: usize,
}

struct OwnerPartition {
    key: PartitionKey,
    graph: BitGraph,
    // Indexed by the shared source slot; None denotes a removed source.
    shared_sources: Vec<Option<SlotId>>,
}

/// One shared partition and independent queues for each World owner.
pub(super) struct ReactiveGraph {
    shared: BitGraph,
    owners: Vec<OwnerPartition>,
    capacity: GraphCapacity,
    next_owner_epoch: u64,
}

impl BitGraph {
    const fn new() -> Self {
        Self {
            sources: Vec::new(),
            consumers: Vec::new(),
            rows: Vec::new(),
            pending: VecDeque::new(),
            source_limit: 0,
            consumer_limit: 0,
            words_per_row: 0,
        }
    }

    fn try_new(source_limit: usize, consumer_limit: usize) -> Result<Self, GraphError> {
        if source_limit > u32::MAX as usize || consumer_limit > u32::MAX as usize {
            return Err(GraphError::CapacityOverflow);
        }
        let words_per_row = consumer_limit.div_ceil(64);
        let row_words = source_limit
            .checked_mul(words_per_row)
            .ok_or(GraphError::CapacityOverflow)?;
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(source_limit)
            .map_err(|_| GraphError::Allocation)?;
        let mut consumers = Vec::new();
        consumers
            .try_reserve_exact(consumer_limit)
            .map_err(|_| GraphError::Allocation)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(row_words)
            .map_err(|_| GraphError::Allocation)?;
        rows.resize(row_words, 0);
        let mut pending = VecDeque::new();
        pending
            .try_reserve(consumer_limit)
            .map_err(|_| GraphError::Allocation)?;
        Ok(Self {
            sources,
            consumers,
            rows,
            pending,
            source_limit,
            consumer_limit,
            words_per_row,
        })
    }

    fn try_grown(&self, source_limit: usize, consumer_limit: usize) -> Result<Self, GraphError> {
        if source_limit < self.source_limit || consumer_limit < self.consumer_limit {
            return Err(GraphError::CapacityOverflow);
        }
        let mut grown = Self::try_new(source_limit, consumer_limit)?;
        grown.sources.extend_from_slice(&self.sources);
        grown.consumers.extend_from_slice(&self.consumers);
        for source in 0..self.sources.len() {
            let old = self.row(source);
            let start = source * grown.words_per_row;
            grown.rows[start..start + old.len()].copy_from_slice(old);
        }
        grown.pending.extend(self.pending.iter().copied());
        Ok(grown)
    }

    fn can_add_source(&self) -> bool {
        self.sources
            .iter()
            .any(|node| !node.live && node.generation < u32::MAX)
            || self.sources.len() < self.source_limit
    }

    fn can_add_consumer(&self) -> bool {
        self.consumers
            .iter()
            .any(|node| !node.live && node.generation < u32::MAX)
            || self.consumers.len() < self.consumer_limit
    }

    fn available_consumer_slots(&self) -> usize {
        self.consumers
            .iter()
            .filter(|node| !node.live && node.generation < u32::MAX)
            .count()
            + self.consumer_limit
            - self.consumers.len()
    }

    fn add_source(&mut self, partition: PartitionKey) -> Result<SlotId, GraphError> {
        let id = if let Some((slot, node)) = self
            .sources
            .iter_mut()
            .enumerate()
            .find(|(_, node)| !node.live && node.generation < u32::MAX)
        {
            node.generation += 1;
            node.live = true;
            SlotId {
                slot: slot as u32,
                generation: node.generation,
            }
        } else {
            if self.sources.len() == self.source_limit {
                return Err(GraphError::Full {
                    partition,
                    kind: CapacityKind::Sources,
                    limit: self.source_limit,
                });
            }
            let slot = self.sources.len() as u32;
            self.sources.push(SourceNode {
                generation: 1,
                live: true,
            });
            SlotId {
                slot,
                generation: 1,
            }
        };
        self.row_mut(id.slot as usize).fill(0);
        Ok(id)
    }

    fn add_consumer(
        &mut self,
        partition: PartitionKey,
        target: Subscriber,
        automatic: bool,
    ) -> Result<SlotId, GraphError> {
        let id = if let Some((slot, node)) = self
            .consumers
            .iter_mut()
            .enumerate()
            .find(|(_, node)| !node.live && node.generation < u32::MAX)
        {
            node.generation += 1;
            node.live = true;
            node.pending = false;
            node.target = target;
            node.automatic = automatic;
            SlotId {
                slot: slot as u32,
                generation: node.generation,
            }
        } else {
            if self.consumers.len() == self.consumer_limit {
                return Err(GraphError::Full {
                    partition,
                    kind: CapacityKind::Consumers,
                    limit: self.consumer_limit,
                });
            }
            let slot = self.consumers.len() as u32;
            self.consumers.push(ConsumerNode {
                generation: 1,
                live: true,
                pending: false,
                target,
                automatic,
            });
            SlotId {
                slot,
                generation: 1,
            }
        };
        self.clear_column(id.slot as usize);
        Ok(id)
    }

    fn source_is_live(&self, id: SlotId) -> bool {
        self.sources
            .get(id.slot as usize)
            .is_some_and(|node| node.live && node.generation == id.generation)
    }

    fn consumer_is_live(&self, id: SlotId) -> bool {
        self.consumers
            .get(id.slot as usize)
            .is_some_and(|node| node.live && node.generation == id.generation)
    }

    fn remove_source(&mut self, id: SlotId) -> Result<(), GraphError> {
        if !self.source_is_live(id) {
            return Err(GraphError::StaleSource);
        }
        self.sources[id.slot as usize].live = false;
        self.row_mut(id.slot as usize).fill(0);
        Ok(())
    }

    fn remove_consumer(&mut self, id: SlotId) -> Result<(), GraphError> {
        self.clear_consumer(id)?;
        let node = &mut self.consumers[id.slot as usize];
        node.live = false;
        node.pending = false;
        self.pending.retain(|queued| *queued != id);
        Ok(())
    }

    fn subscribe(&mut self, source: SlotId, consumer: SlotId) -> Result<(), GraphError> {
        if !self.source_is_live(source) {
            return Err(GraphError::StaleSource);
        }
        if !self.consumer_is_live(consumer) {
            return Err(GraphError::StaleConsumer);
        }
        let index = source.slot as usize * self.words_per_row + consumer.slot as usize / 64;
        self.rows[index] |= 1u64 << (consumer.slot % 64);
        Ok(())
    }

    fn clear_consumer(&mut self, consumer: SlotId) -> Result<(), GraphError> {
        if !self.consumer_is_live(consumer) {
            return Err(GraphError::StaleConsumer);
        }
        self.clear_column(consumer.slot as usize);
        Ok(())
    }

    fn clear_column(&mut self, consumer_slot: usize) {
        if self.words_per_row == 0 {
            return;
        }
        let word = consumer_slot / 64;
        let mask = !(1u64 << (consumer_slot % 64));
        for source in 0..self.sources.len() {
            self.rows[source * self.words_per_row + word] &= mask;
        }
    }

    fn row(&self, source_slot: usize) -> &[u64] {
        let start = source_slot * self.words_per_row;
        &self.rows[start..start + self.words_per_row]
    }

    fn row_mut(&mut self, source_slot: usize) -> &mut [u64] {
        let start = source_slot * self.words_per_row;
        &mut self.rows[start..start + self.words_per_row]
    }

    #[allow(dead_code)] // The production traversal API currently has no runtime caller.
    fn for_each_subscriber(
        &self,
        source: SlotId,
        partition: PartitionKey,
        visit: &mut impl FnMut(ConsumerKey, Subscriber),
    ) -> Result<(), GraphError> {
        if !self.source_is_live(source) {
            return Err(GraphError::StaleSource);
        }
        for (word, bits) in self.row(source.slot as usize).iter().copied().enumerate() {
            let mut bits = bits;
            while bits != 0 {
                let slot = word * 64 + bits.trailing_zeros() as usize;
                if let Some(node) = self.consumers.get(slot)
                    && node.live
                {
                    visit(
                        ConsumerKey {
                            partition,
                            slot: SlotId {
                                slot: slot as u32,
                                generation: node.generation,
                            },
                        },
                        node.target,
                    );
                }
                bits &= bits - 1;
            }
        }
        Ok(())
    }

    fn check_publish(
        &self,
        source: SlotId,
        partition: PartitionKey,
        kind: PendingKind,
    ) -> Result<(), GraphError> {
        if !self.source_is_live(source) {
            return Err(GraphError::StaleSource);
        }
        let mut needed = 0usize;
        for (word, bits) in self.row(source.slot as usize).iter().copied().enumerate() {
            let mut bits = bits;
            while bits != 0 {
                let slot = word * 64 + bits.trailing_zeros() as usize;
                if self
                    .consumers
                    .get(slot)
                    .is_some_and(|node| node.live && !node.pending && kind.matches(node.target))
                {
                    needed += 1;
                }
                bits &= bits - 1;
            }
        }
        if self
            .pending
            .len()
            .checked_add(needed)
            .is_none_or(|count| count > self.consumer_limit)
        {
            return Err(GraphError::Full {
                partition,
                kind: CapacityKind::Pending,
                limit: self.consumer_limit,
            });
        }
        Ok(())
    }

    // check_publish has already verified capacity. This method does not call user code.
    fn publish_checked(&mut self, source: SlotId, kind: PendingKind) {
        for word in 0..self.words_per_row {
            let mut bits = self.rows[source.slot as usize * self.words_per_row + word];
            while bits != 0 {
                let slot = word * 64 + bits.trailing_zeros() as usize;
                if let Some(node) = self.consumers.get_mut(slot)
                    && node.live
                    && !node.pending
                    && kind.matches(node.target)
                {
                    node.pending = true;
                    self.pending.push_back(SlotId {
                        slot: slot as u32,
                        generation: node.generation,
                    });
                }
                bits &= bits - 1;
            }
        }
    }

    fn take_pending_kind(&mut self, kind: PendingKind) -> Option<(SlotId, Subscriber)> {
        let mut index = 0;
        while index < self.pending.len() {
            let id = self.pending[index];
            let Some(node) = self.consumers.get(id.slot as usize) else {
                self.pending.remove(index);
                continue;
            };
            if !node.live || node.generation != id.generation {
                self.pending.remove(index);
                continue;
            }
            if kind.matches(node.target) {
                self.pending.remove(index);
                let node = &mut self.consumers[id.slot as usize];
                node.pending = false;
                return Some((id, node.target));
            }
            index += 1;
        }
        None
    }

    #[cfg(test)]
    fn reserved_bytes(&self) -> usize {
        self.sources.capacity() * ::core::mem::size_of::<SourceNode>()
            + self.consumers.capacity() * ::core::mem::size_of::<ConsumerNode>()
            + self.rows.capacity() * ::core::mem::size_of::<u64>()
            + self.pending.capacity() * ::core::mem::size_of::<SlotId>()
    }
}

impl ReactiveGraph {
    /// A zero-capacity graph suitable for the ambient runtime's const setup.
    /// Registration requires an explicit structural reserve first.
    pub const fn new() -> Self {
        Self {
            shared: BitGraph::new(),
            owners: Vec::new(),
            capacity: GraphCapacity::new(0, 0, 0, 0, 0),
            next_owner_epoch: 1,
        }
    }

    #[cfg(test)]
    pub fn try_new(capacity: GraphCapacity) -> Result<Self, GraphError> {
        let mut graph = Self::new();
        graph.reserve(capacity)?;
        Ok(graph)
    }

    #[cfg(test)]
    pub fn capacity(&self) -> GraphCapacity {
        self.capacity
    }

    /// Explicit structural growth. All replacement buffers are prepared
    /// before any node, edge, queue, proxy, or logical limit is changed.
    pub fn reserve(&mut self, requested: GraphCapacity) -> Result<(), GraphError> {
        let next = GraphCapacity {
            shared_sources: self.capacity.shared_sources.max(requested.shared_sources),
            shared_consumers: self
                .capacity
                .shared_consumers
                .max(requested.shared_consumers),
            owner_sources: self.capacity.owner_sources.max(requested.owner_sources),
            owner_consumers: self.capacity.owner_consumers.max(requested.owner_consumers),
            owners: self.capacity.owners.max(requested.owners),
        };
        if next == self.capacity {
            return Ok(());
        }
        let shared = if next.shared_sources != self.capacity.shared_sources
            || next.shared_consumers != self.capacity.shared_consumers
        {
            Some(
                self.shared
                    .try_grown(next.shared_sources, next.shared_consumers)?,
            )
        } else {
            None
        };
        let grow_owner_graph = next.owner_sources != self.capacity.owner_sources
            || next.owner_consumers != self.capacity.owner_consumers;
        let grow_proxies = next.shared_sources != self.capacity.shared_sources;
        let mut owner_replacements = Vec::new();
        if grow_owner_graph || grow_proxies {
            owner_replacements
                .try_reserve_exact(self.owners.len())
                .map_err(|_| GraphError::Allocation)?;
            for owner in &self.owners {
                let graph = if grow_owner_graph {
                    Some(
                        owner
                            .graph
                            .try_grown(next.owner_sources, next.owner_consumers)?,
                    )
                } else {
                    None
                };
                let shared_sources = if grow_proxies {
                    let mut links = Vec::new();
                    links
                        .try_reserve_exact(next.shared_sources)
                        .map_err(|_| GraphError::Allocation)?;
                    links.extend_from_slice(&owner.shared_sources);
                    Some(links)
                } else {
                    None
                };
                owner_replacements.push((graph, shared_sources));
            }
        }
        if next.owners != self.capacity.owners {
            self.owners
                .try_reserve_exact(next.owners.saturating_sub(self.owners.len()))
                .map_err(|_| GraphError::Allocation)?;
        }
        if let Some(shared) = shared {
            self.shared = shared;
        }
        for (owner, (graph, shared_sources)) in self.owners.iter_mut().zip(owner_replacements) {
            if let Some(graph) = graph {
                owner.graph = graph;
            }
            if let Some(shared_sources) = shared_sources {
                owner.shared_sources = shared_sources;
            }
        }
        self.capacity = next;
        Ok(())
    }

    /// Prepare one more shared source and its proxy in every existing owner.
    pub fn reserve_for_shared_source(&mut self) -> Result<(), GraphError> {
        let mut next = self.capacity;
        if !self.shared.can_add_source() {
            next.shared_sources = next
                .shared_sources
                .checked_add(1)
                .ok_or(GraphError::CapacityOverflow)?;
        }
        if self
            .owners
            .iter()
            .any(|owner| !owner.graph.can_add_source())
        {
            next.owner_sources = next
                .owner_sources
                .checked_add(1)
                .ok_or(GraphError::CapacityOverflow)?;
        }
        self.reserve(next)
    }

    /// Prepare one more local source for an already registered owner.
    pub fn reserve_for_owner_source(&mut self, world: WorldId) -> Result<(), GraphError> {
        let key = self.owner_key(world)?;
        if self
            .owner_by_key(key)
            .expect("owner key just resolved")
            .graph
            .can_add_source()
        {
            return Ok(());
        }
        let mut next = self.capacity;
        next.owner_sources = next
            .owner_sources
            .checked_add(1)
            .ok_or(GraphError::CapacityOverflow)?;
        self.reserve(next)
    }

    /// Prepare one more consumer in its fixed partition.
    pub fn reserve_for_consumer(&mut self, partition: PartitionKey) -> Result<(), GraphError> {
        let can_add = match partition {
            PartitionKey::Shared => self.shared.can_add_consumer(),
            PartitionKey::Owner { .. } => self
                .owner_by_key(partition)
                .ok_or(GraphError::StaleConsumer)?
                .graph
                .can_add_consumer(),
        };
        if can_add {
            return Ok(());
        }
        let mut next = self.capacity;
        match partition {
            PartitionKey::Shared => {
                next.shared_consumers = next
                    .shared_consumers
                    .checked_add(1)
                    .ok_or(GraphError::CapacityOverflow)?;
            }
            PartitionKey::Owner { .. } => {
                next.owner_consumers = next
                    .owner_consumers
                    .checked_add(1)
                    .ok_or(GraphError::CapacityOverflow)?;
            }
        }
        self.reserve(next)
    }

    /// Keep this many unclaimed consumer slots in every owner partition.
    /// Structural registration calls this before adding owned consumers so
    /// an ownerless Effect can bind to its first model without heap growth.
    pub fn reserve_owner_consumer_headroom(&mut self, headroom: usize) -> Result<(), GraphError> {
        let deficit = self
            .owners
            .iter()
            .map(|owner| headroom.saturating_sub(owner.graph.available_consumer_slots()))
            .max()
            .unwrap_or(0)
            .max(headroom.saturating_sub(self.capacity.owner_consumers));
        if deficit == 0 {
            return Ok(());
        }
        let mut next = self.capacity;
        next.owner_consumers = next
            .owner_consumers
            .checked_add(deficit)
            .ok_or(GraphError::CapacityOverflow)?;
        self.reserve(next)
    }

    /// Prepare one more owner, including proxies for existing shared sources.
    pub fn reserve_for_owner(&mut self) -> Result<(), GraphError> {
        let mut next = self.capacity;
        if self.owners.len() == next.owners {
            next.owners = next
                .owners
                .checked_add(1)
                .ok_or(GraphError::CapacityOverflow)?;
        }
        next.owner_sources = next.owner_sources.max(
            self.shared
                .sources
                .iter()
                .filter(|source| source.live)
                .count(),
        );
        self.reserve(next)
    }

    pub fn owner_key(&self, world: WorldId) -> Result<PartitionKey, GraphError> {
        self.owners
            .iter()
            .find(|owner| matches!(owner.key, PartitionKey::Owner { world: id, .. } if id == world))
            .map(|owner| owner.key)
            .ok_or(GraphError::MissingOwner(world))
    }

    pub fn add_owner(&mut self, world: WorldId) -> Result<PartitionKey, GraphError> {
        if self.owner_key(world).is_ok() {
            return Err(GraphError::DuplicateOwner(world));
        }
        if self.owners.len() == self.capacity.owners {
            return Err(GraphError::Full {
                partition: PartitionKey::Shared,
                kind: CapacityKind::Owners,
                limit: self.capacity.owners,
            });
        }
        let next_epoch = self
            .next_owner_epoch
            .checked_add(1)
            .ok_or(GraphError::IdentityExhausted)?;
        let key = PartitionKey::Owner {
            world,
            epoch: self.next_owner_epoch,
        };
        if self
            .shared
            .sources
            .iter()
            .filter(|source| source.live)
            .count()
            > self.capacity.owner_sources
        {
            return Err(GraphError::Full {
                partition: key,
                kind: CapacityKind::Sources,
                limit: self.capacity.owner_sources,
            });
        }
        let mut graph =
            BitGraph::try_new(self.capacity.owner_sources, self.capacity.owner_consumers)?;
        let mut shared_sources = Vec::new();
        shared_sources
            .try_reserve_exact(self.capacity.shared_sources)
            .map_err(|_| GraphError::Allocation)?;
        shared_sources.resize(self.shared.sources.len(), None);
        for (slot, source) in self.shared.sources.iter().enumerate() {
            if source.live {
                let proxy = graph
                    .add_source(key)
                    .expect("shared proxy preflighted against owner source capacity");
                shared_sources[slot] = Some(proxy);
            }
        }
        self.owners.push(OwnerPartition {
            key,
            graph,
            shared_sources,
        });
        self.next_owner_epoch = next_epoch;
        Ok(key)
    }

    pub fn remove_owner(&mut self, world: WorldId) -> bool {
        let Some(index) = self.owners.iter().position(
            |owner| matches!(owner.key, PartitionKey::Owner { world: id, .. } if id == world),
        ) else {
            return false;
        };
        self.owners.swap_remove(index);
        true
    }

    pub fn register_shared_source(&mut self) -> Result<SourceKey, GraphError> {
        if !self.shared.can_add_source() {
            return Err(GraphError::Full {
                partition: PartitionKey::Shared,
                kind: CapacityKind::Sources,
                limit: self.capacity.shared_sources,
            });
        }
        for owner in &self.owners {
            if !owner.graph.can_add_source() {
                return Err(GraphError::Full {
                    partition: owner.key,
                    kind: CapacityKind::Sources,
                    limit: self.capacity.owner_sources,
                });
            }
        }
        let slot = self.shared.add_source(PartitionKey::Shared)?;
        for owner in &mut self.owners {
            let proxy = owner
                .graph
                .add_source(owner.key)
                .expect("shared proxy registration preflighted");
            let index = slot.slot as usize;
            if index == owner.shared_sources.len() {
                owner.shared_sources.push(Some(proxy));
            } else {
                owner.shared_sources[index] = Some(proxy);
            }
        }
        Ok(SourceKey {
            partition: PartitionKey::Shared,
            slot,
        })
    }

    pub fn register_owner_source(&mut self, world: WorldId) -> Result<SourceKey, GraphError> {
        let owner = self.owner_by_world_mut(world)?;
        let slot = owner.graph.add_source(owner.key)?;
        Ok(SourceKey {
            partition: owner.key,
            slot,
        })
    }

    pub fn remove_source(&mut self, source: SourceKey) -> Result<(), GraphError> {
        match source.partition {
            PartitionKey::Shared => {
                if !self.shared.source_is_live(source.slot) {
                    return Err(GraphError::StaleSource);
                }
                for owner in &self.owners {
                    let proxy = owner
                        .shared_sources
                        .get(source.slot.slot as usize)
                        .and_then(|id| *id)
                        .ok_or(GraphError::StaleSource)?;
                    if !owner.graph.source_is_live(proxy) {
                        return Err(GraphError::StaleSource);
                    }
                }
                self.shared.remove_source(source.slot)?;
                for owner in &mut self.owners {
                    let proxy = owner.shared_sources[source.slot.slot as usize]
                        .take()
                        .expect("shared proxy validated");
                    owner.graph.remove_source(proxy)?;
                }
                Ok(())
            }
            PartitionKey::Owner { .. } => self
                .owner_by_key_mut(source.partition)
                .ok_or(GraphError::StaleSource)?
                .graph
                .remove_source(source.slot),
        }
    }

    pub fn register_consumer(
        &mut self,
        partition: PartitionKey,
        target: Subscriber,
    ) -> Result<ConsumerKey, GraphError> {
        self.register_consumer_with_mode(partition, target, true)
    }

    pub fn register_explicit_visual_consumer(
        &mut self,
        partition: PartitionKey,
        entity: Entity,
    ) -> Result<ConsumerKey, GraphError> {
        self.register_consumer_with_mode(partition, Subscriber::VisualWidget(entity), false)
    }

    fn register_consumer_with_mode(
        &mut self,
        partition: PartitionKey,
        target: Subscriber,
        automatic: bool,
    ) -> Result<ConsumerKey, GraphError> {
        if partition == PartitionKey::Shared
            && matches!(target, Subscriber::Widget(_) | Subscriber::VisualWidget(_))
        {
            return Err(GraphError::UnownedWidget);
        }
        let graph = self
            .partition_mut(partition)
            .ok_or(GraphError::StaleConsumer)?;
        let slot = graph.add_consumer(partition, target, automatic)?;
        Ok(ConsumerKey { partition, slot })
    }

    /// Find a live consumer without allocating a separate target index.
    pub fn find_consumer(
        &self,
        partition: PartitionKey,
        target: Subscriber,
    ) -> Option<ConsumerKey> {
        let graph = self.partition(partition)?;
        graph
            .consumers
            .iter()
            .enumerate()
            .find(|(_, node)| node.live && node.automatic && node.target == target)
            .map(|(slot, node)| ConsumerKey {
                partition,
                slot: SlotId {
                    slot: slot as u32,
                    generation: node.generation,
                },
            })
    }

    pub fn find_consumer_matching(
        &self,
        partition: PartitionKey,
        mut matches: impl FnMut(Subscriber) -> bool,
    ) -> Option<ConsumerKey> {
        let graph = self.partition(partition)?;
        graph
            .consumers
            .iter()
            .enumerate()
            .find(|(_, node)| node.live && matches(node.target))
            .map(|(slot, node)| ConsumerKey {
                partition,
                slot: SlotId {
                    slot: slot as u32,
                    generation: node.generation,
                },
            })
    }

    pub fn count_consumers_matching(&self, mut matches: impl FnMut(Subscriber) -> bool) -> usize {
        self.shared
            .consumers
            .iter()
            .chain(
                self.owners
                    .iter()
                    .flat_map(|owner| owner.graph.consumers.iter()),
            )
            .filter(|node| node.live && matches(node.target))
            .count()
    }

    pub fn clear_consumer(&mut self, consumer: ConsumerKey) -> Result<(), GraphError> {
        self.partition_mut(consumer.partition)
            .ok_or(GraphError::StaleConsumer)?
            .clear_consumer(consumer.slot)
    }

    pub fn remove_consumer(&mut self, consumer: ConsumerKey) -> Result<(), GraphError> {
        self.partition_mut(consumer.partition)
            .ok_or(GraphError::StaleConsumer)?
            .remove_consumer(consumer.slot)
    }

    pub fn subscribe(
        &mut self,
        source: SourceKey,
        consumer: ConsumerKey,
    ) -> Result<(), GraphError> {
        match (source.partition, consumer.partition) {
            (PartitionKey::Shared, PartitionKey::Shared) => {
                self.shared.subscribe(source.slot, consumer.slot)
            }
            (PartitionKey::Shared, PartitionKey::Owner { .. }) => {
                if !self.shared.source_is_live(source.slot) {
                    return Err(GraphError::StaleSource);
                }
                let owner = self
                    .owner_by_key_mut(consumer.partition)
                    .ok_or(GraphError::StaleConsumer)?;
                let proxy = owner
                    .shared_sources
                    .get(source.slot.slot as usize)
                    .and_then(|id| *id)
                    .ok_or(GraphError::StaleSource)?;
                owner.graph.subscribe(proxy, consumer.slot)
            }
            (PartitionKey::Owner { .. }, PartitionKey::Owner { .. })
                if source.partition == consumer.partition =>
            {
                self.owner_by_key_mut(source.partition)
                    .ok_or(GraphError::StaleSource)?
                    .graph
                    .subscribe(source.slot, consumer.slot)
            }
            _ => Err(GraphError::CrossOwner {
                source: source.partition,
                consumer: consumer.partition,
            }),
        }
    }

    pub fn publish(&mut self, source: SourceKey) -> Result<(), GraphError> {
        self.publish_kind(source, PendingKind::Any)
    }

    /// Invalidate cached computations without scheduling user callbacks for
    /// a source whose owner is being destroyed.
    pub fn publish_computed_only(&mut self, source: SourceKey) -> Result<(), GraphError> {
        self.publish_kind(source, PendingKind::Computed)
    }

    fn publish_kind(&mut self, source: SourceKey, kind: PendingKind) -> Result<(), GraphError> {
        match source.partition {
            PartitionKey::Shared => {
                self.shared
                    .check_publish(source.slot, PartitionKey::Shared, kind)?;
                for owner in &self.owners {
                    let proxy = owner
                        .shared_sources
                        .get(source.slot.slot as usize)
                        .and_then(|id| *id)
                        .ok_or(GraphError::StaleSource)?;
                    owner.graph.check_publish(proxy, owner.key, kind)?;
                }
                self.shared.publish_checked(source.slot, kind);
                for owner in &mut self.owners {
                    let proxy = owner.shared_sources[source.slot.slot as usize]
                        .expect("shared proxy validated before publish");
                    owner.graph.publish_checked(proxy, kind);
                }
                Ok(())
            }
            PartitionKey::Owner { .. } => {
                let owner = self
                    .owner_by_key_mut(source.partition)
                    .ok_or(GraphError::StaleSource)?;
                owner.graph.check_publish(source.slot, owner.key, kind)?;
                owner.graph.publish_checked(source.slot, kind);
                Ok(())
            }
        }
    }

    /// Visit live subscribers without building a scratch collection. The
    /// visitor runs while the graph is immutably borrowed; it must not reenter
    /// the ambient reactive runtime or run a user evaluation callback.
    #[allow(dead_code)] // Available for allocation-free source inspection.
    pub fn for_each_subscriber(
        &self,
        source: SourceKey,
        mut visit: impl FnMut(ConsumerKey, Subscriber),
    ) -> Result<(), GraphError> {
        match source.partition {
            PartitionKey::Shared => {
                if !self.shared.source_is_live(source.slot) {
                    return Err(GraphError::StaleSource);
                }
                for owner in &self.owners {
                    let proxy = owner
                        .shared_sources
                        .get(source.slot.slot as usize)
                        .and_then(|id| *id)
                        .ok_or(GraphError::StaleSource)?;
                    if !owner.graph.source_is_live(proxy) {
                        return Err(GraphError::StaleSource);
                    }
                }
                self.shared
                    .for_each_subscriber(source.slot, PartitionKey::Shared, &mut visit)?;
                for owner in &self.owners {
                    let proxy = owner.shared_sources[source.slot.slot as usize]
                        .expect("shared proxy validated before traversal");
                    owner
                        .graph
                        .for_each_subscriber(proxy, owner.key, &mut visit)?;
                }
                Ok(())
            }
            PartitionKey::Owner { .. } => self
                .owners
                .iter()
                .find(|owner| owner.key == source.partition)
                .ok_or(GraphError::StaleSource)?
                .graph
                .for_each_subscriber(source.slot, source.partition, &mut visit),
        }
    }

    #[cfg(test)]
    pub fn take_pending(
        &mut self,
        partition: PartitionKey,
    ) -> Result<Option<(ConsumerKey, Subscriber)>, GraphError> {
        self.take_pending_kind(partition, PendingKind::Any)
    }

    /// Remove one pending item of the requested kind, leaving other kinds in
    /// the bounded queue. In particular, computed invalidations can be drained
    /// before effects or widgets are flushed.
    #[cfg(test)]
    pub fn take_pending_kind(
        &mut self,
        partition: PartitionKey,
        kind: PendingKind,
    ) -> Result<Option<(ConsumerKey, Subscriber)>, GraphError> {
        let graph = self
            .partition_mut(partition)
            .ok_or(GraphError::StaleConsumer)?;
        Ok(graph
            .take_pending_kind(kind)
            .map(|(slot, target)| (ConsumerKey { partition, slot }, target)))
    }

    /// Remove one matching item from any partition without scratch storage.
    pub fn take_pending_any_kind(
        &mut self,
        kind: PendingKind,
    ) -> Option<(ConsumerKey, Subscriber)> {
        if let Some((slot, target)) = self.shared.take_pending_kind(kind) {
            return Some((
                ConsumerKey {
                    partition: PartitionKey::Shared,
                    slot,
                },
                target,
            ));
        }
        for owner in &mut self.owners {
            if let Some((slot, target)) = owner.graph.take_pending_kind(kind) {
                return Some((
                    ConsumerKey {
                        partition: owner.key,
                        slot,
                    },
                    target,
                ));
            }
        }
        None
    }

    /// Find one computed in any partition. The caller must release its runtime
    /// borrow before touching a computed value or publishing its result source.
    pub fn take_pending_computed(&mut self) -> Option<(ConsumerKey, Subscriber)> {
        self.take_pending_any_kind(PendingKind::Computed)
    }

    #[cfg(test)]
    pub fn reserved_bytes(&self) -> usize {
        self.shared.reserved_bytes()
            + self.owners.capacity() * ::core::mem::size_of::<OwnerPartition>()
            + self
                .owners
                .iter()
                .map(|owner| {
                    owner.graph.reserved_bytes()
                        + owner.shared_sources.capacity() * ::core::mem::size_of::<Option<SlotId>>()
                })
                .sum::<usize>()
    }

    fn owner_by_world_mut(&mut self, world: WorldId) -> Result<&mut OwnerPartition, GraphError> {
        self.owners
            .iter_mut()
            .find(|owner| matches!(owner.key, PartitionKey::Owner { world: id, .. } if id == world))
            .ok_or(GraphError::MissingOwner(world))
    }

    fn owner_by_key_mut(&mut self, key: PartitionKey) -> Option<&mut OwnerPartition> {
        self.owners.iter_mut().find(|owner| owner.key == key)
    }

    fn owner_by_key(&self, key: PartitionKey) -> Option<&OwnerPartition> {
        self.owners.iter().find(|owner| owner.key == key)
    }

    fn partition_mut(&mut self, key: PartitionKey) -> Option<&mut BitGraph> {
        match key {
            PartitionKey::Shared => Some(&mut self.shared),
            PartitionKey::Owner { .. } => self.owner_by_key_mut(key).map(|owner| &mut owner.graph),
        }
    }

    fn partition(&self, key: PartitionKey) -> Option<&BitGraph> {
        match key {
            PartitionKey::Shared => Some(&self.shared),
            PartitionKey::Owner { .. } => self.owner_by_key(key).map(|owner| &owner.graph),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    extern crate std;

    use super::*;
    use crate::ecs::{Entity, World};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct CountingAllocator;

    std::thread_local! {
        static TRACK: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = TRACK.try_with(|track| {
                if track.get() {
                    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
                }
            });
            // SAFETY: Forward the unchanged layout to the system allocator.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: The pointer and layout came from the system allocator.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let _ = TRACK.try_with(|track| {
                if track.get() {
                    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
                }
            });
            // SAFETY: Forward the owned allocation and new size unchanged.
            unsafe { System.realloc(ptr, layout, size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    pub(in crate::core::reactive) fn tracked_allocations(run: impl FnOnce()) -> usize {
        ALLOCATIONS.with(|count| count.set(0));
        TRACK.with(|track| track.set(true));
        struct StopTracking;
        impl Drop for StopTracking {
            fn drop(&mut self) {
                TRACK.with(|track| track.set(false));
            }
        }
        let stop = StopTracking;
        run();
        drop(stop);
        ALLOCATIONS.with(Cell::get)
    }

    fn entity(index: u32) -> Entity {
        let mut world = World::new();
        let mut entity = world.spawn_empty();
        for _ in 0..index {
            entity = world.spawn_empty();
        }
        entity
    }

    #[test]
    fn shared_source_fans_out_to_distinct_owner_queues() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 1, 2, 2, 2)).unwrap();
        let mut a = World::new();
        let mut b = World::new();
        let a_widget = a.spawn_empty();
        let b_widget = b.spawn_empty();
        assert_eq!(a_widget, b_widget);
        let a_key = graph.add_owner(a.id()).unwrap();
        let b_key = graph.add_owner(b.id()).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let ca = graph
            .register_consumer(a_key, Subscriber::Widget(a_widget))
            .unwrap();
        let cb = graph
            .register_consumer(b_key, Subscriber::Widget(b_widget))
            .unwrap();
        graph.subscribe(shared, ca).unwrap();
        graph.subscribe(shared, cb).unwrap();
        graph.publish(shared).unwrap();
        assert_eq!(
            graph.take_pending(a_key).unwrap(),
            Some((ca, Subscriber::Widget(a_widget)))
        );
        assert_eq!(graph.take_pending(a_key).unwrap(), None);
        assert_eq!(
            graph.take_pending(b_key).unwrap(),
            Some((cb, Subscriber::Widget(b_widget)))
        );
    }

    #[test]
    fn first_registered_branch_switch_reuses_dependency_storage() {
        let mut graph = ReactiveGraph::new();
        let world = World::new();
        graph.reserve_for_owner().unwrap();
        let owner = graph.add_owner(world.id()).unwrap();
        graph.reserve_for_owner_source(world.id()).unwrap();
        let first = graph.register_owner_source(world.id()).unwrap();
        graph.reserve_for_owner_source(world.id()).unwrap();
        let second = graph.register_owner_source(world.id()).unwrap();
        graph.reserve_for_consumer(owner).unwrap();
        let consumer = graph
            .register_consumer(
                owner,
                Subscriber::Effect(super::super::EffectId(SlotId {
                    slot: 0,
                    generation: 1,
                })),
            )
            .unwrap();
        graph.subscribe(first, consumer).unwrap();
        let reserved = graph.reserved_bytes();
        let allocations = tracked_allocations(|| {
            graph.clear_consumer(consumer).unwrap();
            graph.subscribe(second, consumer).unwrap();
            graph.publish(first).unwrap();
            assert_eq!(graph.take_pending(owner).unwrap(), None);
            graph.publish(second).unwrap();
            assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumer);
        });
        assert_eq!(allocations, 0);
        assert_eq!(graph.reserved_bytes(), reserved);
    }

    #[test]
    fn zero_capacity_const_graph_grows_only_at_explicit_structural_points() {
        let mut graph = ReactiveGraph::new();
        assert_eq!(graph.capacity(), GraphCapacity::new(0, 0, 0, 0, 0));
        assert_eq!(
            graph.register_shared_source(),
            Err(GraphError::Full {
                partition: PartitionKey::Shared,
                kind: CapacityKind::Sources,
                limit: 0,
            })
        );
        graph.reserve_for_shared_source().unwrap();
        let shared = graph.register_shared_source().unwrap();
        graph.reserve_for_owner().unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        graph.reserve_for_consumer(owner).unwrap();
        let widget = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(shared, widget).unwrap();
        graph.publish(shared).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, widget);
        assert_eq!(graph.capacity(), GraphCapacity::new(1, 0, 1, 1, 1));
    }

    #[test]
    fn owner_consumer_headroom_survives_registration_without_hot_growth() {
        let mut graph = ReactiveGraph::new();
        let first = World::new();
        let second = World::new();
        graph.reserve_for_owner().unwrap();
        let first_partition = graph.add_owner(first.id()).unwrap();
        graph.reserve_for_owner().unwrap();
        let second_partition = graph.add_owner(second.id()).unwrap();
        graph.reserve_owner_consumer_headroom(2).unwrap();
        let first_target = Subscriber::Widget(entity(1));
        let first_consumer = graph
            .register_consumer(first_partition, first_target)
            .unwrap();
        assert_eq!(
            graph.find_consumer(first_partition, first_target),
            Some(first_consumer)
        );

        graph.reserve_owner_consumer_headroom(2).unwrap();
        let visual_target = Subscriber::VisualWidget(entity(2));
        let second_target = Subscriber::Widget(entity(3));
        let allocations = tracked_allocations(|| {
            graph
                .register_consumer(first_partition, visual_target)
                .unwrap();
            graph
                .register_consumer(second_partition, second_target)
                .unwrap();
        });
        assert_eq!(allocations, 0);
        assert_eq!(
            graph.count_consumers_matching(|target| matches!(target, Subscriber::Widget(_))),
            2
        );
    }

    #[test]
    fn growing_matrix_stride_preserves_edges_pending_flags_and_keys() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 0, 2, 64, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let local = graph.register_owner_source(world.id()).unwrap();
        let mut consumers = Vec::new();
        for index in 0..64 {
            consumers.push(
                graph
                    .register_consumer(owner, Subscriber::Widget(entity(index)))
                    .unwrap(),
            );
        }
        graph.subscribe(shared, consumers[0]).unwrap();
        graph.subscribe(shared, consumers[63]).unwrap();
        graph.subscribe(local, consumers[1]).unwrap();
        graph.publish(shared).unwrap();
        graph.reserve(GraphCapacity::new(2, 0, 3, 65, 1)).unwrap();
        let last = graph
            .register_consumer(owner, Subscriber::Widget(entity(64)))
            .unwrap();
        graph.subscribe(shared, last).unwrap();
        graph.publish(shared).unwrap();
        graph.publish(local).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumers[0]);
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumers[63]);
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, last);
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumers[1]);
        assert_eq!(graph.take_pending(owner).unwrap(), None);
        assert_eq!(graph.capacity(), GraphCapacity::new(2, 0, 3, 65, 1));
    }

    #[test]
    fn shared_source_growth_preserves_full_owner_local_edges() {
        let mut graph = ReactiveGraph::new();
        let world = World::new();
        graph.reserve_for_owner().unwrap();
        let owner = graph.add_owner(world.id()).unwrap();
        graph.reserve_for_owner_source(world.id()).unwrap();
        let local = graph.register_owner_source(world.id()).unwrap();
        graph.reserve_for_consumer(owner).unwrap();
        let consumer = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(local, consumer).unwrap();

        graph.reserve_for_shared_source().unwrap();
        let shared = graph.register_shared_source().unwrap();
        assert_eq!(graph.capacity(), GraphCapacity::new(1, 0, 2, 1, 1));
        graph.publish(local).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumer);
        graph.subscribe(shared, consumer).unwrap();
        graph.publish(shared).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumer);
    }

    #[test]
    fn failed_growth_leaves_edges_keys_queues_and_limits_unchanged() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 0, 1, 1, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let widget = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(shared, widget).unwrap();
        graph.publish(shared).unwrap();
        let before_capacity = graph.capacity();
        let before_bytes = graph.reserved_bytes();
        assert!(matches!(
            graph.reserve(GraphCapacity::new(2, 0, 1, usize::MAX, 1)),
            Err(GraphError::CapacityOverflow | GraphError::Allocation)
        ));
        assert_eq!(graph.capacity(), before_capacity);
        assert_eq!(graph.reserved_bytes(), before_bytes);
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, widget);
        graph.publish(shared).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, widget);
    }

    #[test]
    fn stale_consumer_cannot_clear_reused_edges_or_pending_work() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(0, 0, 1, 1, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let source = graph.register_owner_source(world.id()).unwrap();
        let old = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(source, old).unwrap();
        graph.publish(source).unwrap();
        graph.remove_consumer(old).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap(), None);
        let current = graph
            .register_consumer(owner, Subscriber::Widget(entity(1)))
            .unwrap();
        assert_ne!(old, current);
        graph.subscribe(source, current).unwrap();
        assert_eq!(graph.clear_consumer(old), Err(GraphError::StaleConsumer));
        graph.publish(source).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, current);
    }

    #[test]
    fn shared_registration_checks_every_owner_before_mutating_any_partition() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(2, 0, 1, 0, 2)).unwrap();
        let a = World::new();
        let b = World::new();
        let a_key = graph.add_owner(a.id()).unwrap();
        let b_key = graph.add_owner(b.id()).unwrap();
        let b_source = graph.register_owner_source(b.id()).unwrap();
        let reserved = graph.reserved_bytes();
        assert_eq!(
            graph.register_shared_source(),
            Err(GraphError::Full {
                partition: b_key,
                kind: CapacityKind::Sources,
                limit: 1,
            })
        );
        assert_eq!(graph.shared.sources.len(), 0);
        assert_eq!(graph.owners[0].graph.sources.len(), 0);
        assert_eq!(graph.reserved_bytes(), reserved);
        graph.remove_source(b_source).unwrap();
        let shared = graph.register_shared_source().unwrap();
        assert_eq!(graph.owners[0].key, a_key);
        assert_eq!(graph.owners[1].key, b_key);
        assert_eq!(shared.partition, PartitionKey::Shared);
    }

    #[test]
    fn shared_source_reuse_rejects_old_identity_and_refreshes_proxies() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 0, 1, 1, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let old = graph.register_shared_source().unwrap();
        let consumer = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(old, consumer).unwrap();
        graph.remove_source(old).unwrap();
        let current = graph.register_shared_source().unwrap();
        assert_ne!(old, current);
        assert_eq!(graph.publish(old), Err(GraphError::StaleSource));
        graph.publish(current).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap(), None);
        graph.subscribe(current, consumer).unwrap();
        graph.publish(current).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumer);
    }

    #[test]
    fn removed_owner_invalidates_old_partition_keys() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(0, 0, 1, 1, 1)).unwrap();
        let world = World::new();
        let first = graph.add_owner(world.id()).unwrap();
        let source = graph.register_owner_source(world.id()).unwrap();
        let consumer = graph
            .register_consumer(first, Subscriber::Widget(entity(0)))
            .unwrap();
        assert!(graph.remove_owner(world.id()));
        let second = graph.add_owner(world.id()).unwrap();
        assert_ne!(first, second);
        assert_eq!(graph.publish(source), Err(GraphError::StaleSource));
        assert_eq!(
            graph.clear_consumer(consumer),
            Err(GraphError::StaleConsumer)
        );
        assert_eq!(graph.take_pending(first), Err(GraphError::StaleConsumer));
    }

    #[test]
    fn ownerless_consumers_cannot_subscribe_to_local_sources() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 1, 1, 1, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let local = graph.register_owner_source(world.id()).unwrap();
        let computed = graph
            .register_consumer(
                PartitionKey::Shared,
                Subscriber::Computed(super::super::ComputedId(SlotId {
                    slot: 0,
                    generation: 1,
                })),
            )
            .unwrap();
        assert_eq!(
            graph.subscribe(local, computed),
            Err(GraphError::CrossOwner {
                source: owner,
                consumer: PartitionKey::Shared,
            })
        );
        assert_eq!(
            graph.register_consumer(PartitionKey::Shared, Subscriber::Widget(entity(0))),
            Err(GraphError::UnownedWidget)
        );
    }

    #[test]
    fn shared_source_registered_before_owner_gains_a_live_proxy() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 0, 1, 1, 1)).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let consumer = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(shared, consumer).unwrap();
        graph.publish(shared).unwrap();
        assert_eq!(graph.take_pending(owner).unwrap().unwrap().0, consumer);
    }

    #[test]
    fn subscriber_walk_visits_shared_and_owner_edges_without_scratch_storage() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 1, 1, 1, 2)).unwrap();
        let a = World::new();
        let b = World::new();
        let a_key = graph.add_owner(a.id()).unwrap();
        let b_key = graph.add_owner(b.id()).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let computed = graph
            .register_consumer(
                PartitionKey::Shared,
                Subscriber::Computed(super::super::ComputedId(SlotId {
                    slot: 1,
                    generation: 1,
                })),
            )
            .unwrap();
        let a_consumer = graph
            .register_consumer(a_key, Subscriber::Widget(entity(0)))
            .unwrap();
        let b_consumer = graph
            .register_consumer(b_key, Subscriber::VisualWidget(entity(0)))
            .unwrap();
        for consumer in [computed, a_consumer, b_consumer] {
            graph.subscribe(shared, consumer).unwrap();
        }
        let mut seen = [None; 3];
        let mut used = 0;
        graph
            .for_each_subscriber(shared, |key, target| {
                seen[used] = Some((key, target));
                used += 1;
            })
            .unwrap();
        assert_eq!(used, 3);
        assert_eq!(seen[0].unwrap().0, computed);
        assert_eq!(seen[1].unwrap().0, a_consumer);
        assert_eq!(seen[2].unwrap().0, b_consumer);
        graph.remove_source(shared).unwrap();
        let mut visited = false;
        assert_eq!(
            graph.for_each_subscriber(shared, |_, _| visited = true),
            Err(GraphError::StaleSource)
        );
        assert!(!visited);
    }

    #[test]
    fn computed_work_can_be_taken_before_other_pending_work() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(1, 1, 1, 2, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let shared = graph.register_shared_source().unwrap();
        let local_computed = graph
            .register_consumer(
                owner,
                Subscriber::Computed(super::super::ComputedId(SlotId {
                    slot: 0,
                    generation: 1,
                })),
            )
            .unwrap();
        let widget = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        graph.subscribe(shared, widget).unwrap();
        graph.subscribe(shared, local_computed).unwrap();
        graph.publish(shared).unwrap();
        assert_eq!(graph.take_pending_computed().unwrap().0, local_computed);
        assert_eq!(graph.take_pending_computed(), None);
        assert_eq!(
            graph
                .take_pending_kind(owner, PendingKind::NonComputed)
                .unwrap()
                .unwrap()
                .0,
            widget
        );
        assert_eq!(graph.take_pending(owner).unwrap(), None);
    }

    #[test]
    fn exhausted_source_and_consumer_slots_retire_without_reusing_old_keys() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(0, 0, 2, 2, 1)).unwrap();
        let world = World::new();
        let owner = graph.add_owner(world.id()).unwrap();
        let source = graph.register_owner_source(world.id()).unwrap();
        let consumer = graph
            .register_consumer(owner, Subscriber::Widget(entity(0)))
            .unwrap();
        let partition = &mut graph.owners[0].graph;
        partition.sources[source.slot.slot as usize].generation = u32::MAX;
        partition.consumers[consumer.slot.slot as usize].generation = u32::MAX;
        let last_source = SourceKey {
            partition: owner,
            slot: SlotId {
                slot: source.slot.slot,
                generation: u32::MAX,
            },
        };
        let last_consumer = ConsumerKey {
            partition: owner,
            slot: SlotId {
                slot: consumer.slot.slot,
                generation: u32::MAX,
            },
        };
        graph.remove_source(last_source).unwrap();
        graph.remove_consumer(last_consumer).unwrap();
        let current_source = graph.register_owner_source(world.id()).unwrap();
        let current_consumer = graph
            .register_consumer(owner, Subscriber::Widget(entity(1)))
            .unwrap();
        assert_ne!(current_source.slot.slot, last_source.slot.slot);
        assert_ne!(current_consumer.slot.slot, last_consumer.slot.slot);
        assert_eq!(graph.publish(last_source), Err(GraphError::StaleSource));
        assert_eq!(
            graph.clear_consumer(last_consumer),
            Err(GraphError::StaleConsumer)
        );
        graph.subscribe(current_source, current_consumer).unwrap();
        assert_eq!(
            graph.register_owner_source(world.id()),
            Err(GraphError::Full {
                partition: owner,
                kind: CapacityKind::Sources,
                limit: 2,
            })
        );
    }

    #[test]
    fn local_sources_reject_consumers_from_another_owner() {
        let mut graph = ReactiveGraph::try_new(GraphCapacity::new(0, 0, 1, 1, 2)).unwrap();
        let a = World::new();
        let b = World::new();
        let a_key = graph.add_owner(a.id()).unwrap();
        let b_key = graph.add_owner(b.id()).unwrap();
        let source = graph.register_owner_source(a.id()).unwrap();
        let consumer = graph
            .register_consumer(b_key, Subscriber::Widget(entity(0)))
            .unwrap();
        assert_eq!(
            graph.subscribe(source, consumer),
            Err(GraphError::CrossOwner {
                source: a_key,
                consumer: b_key,
            })
        );
    }
}
