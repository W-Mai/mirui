//! Borrowed access to successfully prepared entity outputs.

use core::cell::{Cell, Ref, RefCell, RefMut};

use hashbrown::HashMap;

use crate::ecs::Entity;

pub(crate) mod private {
    pub trait OutputKind {}
    pub trait HistoryKind {}
}

/// A supported output kind with storage owned by its resource.
///
/// Reading only projects an already prepared record into its storage. It does not produce data, register demand, or access other resources.
pub trait Output: private::OutputKind + 'static {
    type Storage: 'static;
    type Record: Copy + 'static;
    type Read<'a>
    where
        Self: 'a;

    fn read<'a>(storage: Ref<'a, Self::Storage>, record: Self::Record) -> Option<Self::Read<'a>>;
}

/// An output kind whose producer can retain records and their referenced storage across a production batch.
///
/// The storage adapter keeps the frozen record's payload alive and excludes it from writable slots until a later batch releases that record.
pub trait HistoryOutput: Output + private::HistoryKind {}

/// Successfully published data and its input and publication versions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PublishedOutput<R: Copy> {
    pub(crate) record: R,
    pub(crate) input_version: u64,
    pub(crate) version: u64,
}

/// Copyable metadata used by storage adapters to manage payload ownership.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OutputState<R: Copy> {
    pub(crate) input_version: u64,
    pub(crate) current: Option<PublishedOutput<R>>,
    pub(crate) previous: Option<PublishedOutput<R>>,
    pub(crate) current_available: bool,
    pub(crate) retained: bool,
    pub(crate) published_in_batch: bool,
}

impl<R: Copy> Default for OutputState<R> {
    fn default() -> Self {
        Self {
            input_version: 0,
            current: None,
            previous: None,
            current_available: false,
            retained: false,
            published_in_batch: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputStoreError {
    Allocation,
    Capacity { capacity: usize },
    Missing(Entity),
    StaleInput,
    PublicationExhausted,
}

/// Prepared output records and their uniquely owned payload storage.
///
/// Reads borrow the storage for the lifetime of the returned view. Missing or invalidated current data returns `None`; reads do not request production or allocate memory.
pub struct Outputs<T: Output> {
    storage: RefCell<T::Storage>,
    records: RefCell<HashMap<Entity, OutputState<T::Record>>>,
    next_publication: Cell<u64>,
    batch_depth: Cell<usize>,
}

impl<T: Output> Outputs<T> {
    pub(crate) fn new(storage: T::Storage) -> Self {
        Self {
            storage: RefCell::new(storage),
            records: RefCell::new(HashMap::new()),
            next_publication: Cell::new(1),
            batch_depth: Cell::new(0),
        }
    }

    /// Borrow the current successfully prepared result for the complete entity identity.
    pub fn get(&self, entity: Entity) -> Option<T::Read<'_>> {
        let state = self.state(entity)?;
        let current = state.current?;
        if !state.current_available || current.input_version != state.input_version {
            return None;
        }
        T::read(self.storage.try_borrow().ok()?, current.record)
    }

    /// Grow only the metadata capacity; production and reads never grow it implicitly.
    pub(crate) fn reserve(&self, additional: usize) -> Result<(), OutputStoreError> {
        self.records
            .borrow_mut()
            .try_reserve(additional)
            .map_err(|_| OutputStoreError::Allocation)
    }

    pub(crate) fn storage(&self) -> Ref<'_, T::Storage> {
        self.storage.borrow()
    }

    /// The adapter releases this borrow before calling any consumer or producer callback.
    pub(crate) fn storage_mut(&self) -> RefMut<'_, T::Storage> {
        self.storage.borrow_mut()
    }

    pub(crate) fn state(&self, entity: Entity) -> Option<OutputState<T::Record>> {
        self.records.borrow().get(&entity).copied()
    }

    /// Register an input version and return the prior metadata for payload reconciliation.
    ///
    /// Changed inputs immediately hide current data without discarding its last successful record. A producer with history keeps that record's payload intact; other producers may reclaim it after detaching the writable payload.
    pub(crate) fn prepare(
        &self,
        entity: Entity,
        input_version: u64,
    ) -> Result<Option<OutputState<T::Record>>, OutputStoreError> {
        let mut records = self.records.borrow_mut();
        let previous = records.get(&entity).copied();
        let state = Self::entry(&mut records, entity)?;
        if state.input_version != input_version {
            state.input_version = input_version;
            state.current_available = false;
            state.published_in_batch = false;
        }
        Ok(previous)
    }

    /// Publish completed data without changing this batch's frozen history.
    ///
    /// Returning the replaced metadata lets the storage adapter release records that are no longer current or retained. Stale production cannot replace a newer input version.
    pub(crate) fn publish(
        &self,
        entity: Entity,
        input_version: u64,
        record: T::Record,
    ) -> Result<OutputState<T::Record>, OutputStoreError> {
        let mut records = self.records.borrow_mut();
        let state = records
            .get_mut(&entity)
            .ok_or(OutputStoreError::Missing(entity))?;
        if state.input_version != input_version {
            return Err(OutputStoreError::StaleInput);
        }
        let version = self.next_publication.get();
        let next = version
            .checked_add(1)
            .ok_or(OutputStoreError::PublicationExhausted)?;
        let previous = *state;
        state.current = Some(PublishedOutput {
            record,
            input_version,
            version,
        });
        state.current_available = true;
        state.published_in_batch = true;
        self.next_publication.set(next);
        Ok(previous)
    }

    /// Hide current data before its writable payload is detached or its production fails.
    pub(crate) fn invalidate(&self, entity: Entity) -> Option<OutputState<T::Record>> {
        let mut records = self.records.borrow_mut();
        let state = records.get_mut(&entity)?;
        let previous = *state;
        state.current_available = false;
        state.published_in_batch = false;
        Some(previous)
    }

    /// Clear incompatible or released payload records while preserving the entity's input and history demand.
    pub(crate) fn reset(&self, entity: Entity) -> Option<OutputState<T::Record>> {
        let mut records = self.records.borrow_mut();
        let state = records.get_mut(&entity)?;
        let previous = *state;
        state.current = None;
        state.previous = None;
        state.current_available = false;
        state.published_in_batch = false;
        Some(previous)
    }

    /// Freeze history once for an outer production batch; nested rendering shares that history.
    pub(crate) fn begin_batch(&self) -> OutputBatch<'_, T> {
        let depth = self.batch_depth.get();
        if depth == 0 {
            for state in self.records.borrow_mut().values_mut() {
                let previous = if state.retained { state.current } else { None };
                if state.previous.map(|output| output.version)
                    != previous.map(|output| output.version)
                {
                    state.previous = previous;
                }
                state.published_in_batch = false;
            }
        }
        self.batch_depth.set(depth + 1);
        OutputBatch { outputs: self }
    }

    /// Remove the entity's records; its adapter releases the returned payload references.
    pub(crate) fn release(&self, entity: Entity) -> Option<OutputState<T::Record>> {
        self.records.borrow_mut().remove(&entity)
    }

    pub(crate) fn retain_entities(&self, mut keep: impl FnMut(Entity) -> bool) {
        self.records.borrow_mut().retain(|entity, _| keep(*entity));
    }

    fn entry(
        records: &mut HashMap<Entity, OutputState<T::Record>>,
        entity: Entity,
    ) -> Result<&mut OutputState<T::Record>, OutputStoreError> {
        if !records.contains_key(&entity) && records.len() == records.capacity() {
            return Err(OutputStoreError::Capacity {
                capacity: records.capacity(),
            });
        }
        Ok(records.entry(entity).or_default())
    }
}

impl<T: HistoryOutput> Outputs<T> {
    /// Borrow the previous successful result frozen for the active production batch.
    pub fn previous(&self, entity: Entity) -> Option<T::Read<'_>> {
        let state = self.state(entity)?;
        if !state.retained {
            return None;
        }
        T::read(self.storage.try_borrow().ok()?, state.previous?.record)
    }

    /// Change history demand and return the metadata whose payload pins may need reconciliation.
    pub(crate) fn set_retained(
        &self,
        entity: Entity,
        retained: bool,
    ) -> Result<OutputState<T::Record>, OutputStoreError> {
        let mut records = self.records.borrow_mut();
        let state = Self::entry(&mut records, entity)?;
        let previous = *state;
        if state.retained != retained {
            state.retained = retained;
            state.previous = None;
            if retained && !state.current_available {
                // An unretained producer may have overwritten its invalidated
                // slot, so it cannot become newly retained history.
                state.current = None;
            }
        }
        Ok(previous)
    }
}

#[must_use = "keep the batch guard alive until every output consumer has finished"]
pub(crate) struct OutputBatch<'a, T: Output> {
    outputs: &'a Outputs<T>,
}

impl<T: Output> Drop for OutputBatch<'_, T> {
    fn drop(&mut self) {
        self.outputs
            .batch_depth
            .set(self.outputs.batch_depth.get() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SampleOutput;

    impl private::OutputKind for SampleOutput {}
    impl private::HistoryKind for SampleOutput {}
    impl HistoryOutput for SampleOutput {}

    impl Output for SampleOutput {
        type Storage = [Option<u32>; 4];
        type Record = usize;
        type Read<'a> = Ref<'a, u32>;

        fn read<'a>(storage: Ref<'a, Self::Storage>, record: usize) -> Option<Self::Read<'a>> {
            Ref::filter_map(storage, |storage| storage.get(record)?.as_ref()).ok()
        }
    }

    fn entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    fn outputs() -> Outputs<SampleOutput> {
        let outputs = Outputs::new([Some(10), Some(20), Some(30), None]);
        outputs.reserve(2).unwrap();
        outputs
    }

    #[test]
    fn records_require_explicit_capacity_and_complete_entity_identity() {
        let outputs = Outputs::<SampleOutput>::new([Some(10), None, None, None]);
        let old = entity(7);
        let new = Entity {
            generation: 1,
            ..old
        };
        assert!(matches!(
            outputs.prepare(old, 1),
            Err(OutputStoreError::Capacity { capacity: 0 })
        ));
        outputs.reserve(2).unwrap();
        outputs.prepare(old, 1).unwrap();
        outputs.publish(old, 1, 0).unwrap();
        assert_eq!(*outputs.get(old).unwrap(), 10);
        assert!(outputs.get(new).is_none());
        assert!(outputs.release(new).is_none());
        let released = outputs.release(old).unwrap();
        assert_eq!(released.current.unwrap().record, 0);
        assert!(outputs.get(old).is_none());
        outputs.prepare(new, 1).unwrap();
        assert!(outputs.get(new).is_none());
    }

    #[test]
    fn publication_requires_prepared_inputs_and_success() {
        let outputs = outputs();
        let source = entity(0);
        let consumer = entity(1);
        assert!(matches!(
            outputs.publish(source, 1, 0),
            Err(OutputStoreError::Missing(missing)) if missing == source
        ));
        outputs.prepare(source, 1).unwrap();
        assert!(outputs.get(source).is_none());
        outputs.publish(source, 1, 0).unwrap();
        let first = outputs.state(source).unwrap().current.unwrap();
        assert_eq!(first.input_version, 1);
        outputs.prepare(consumer, 1).unwrap();
        outputs.publish(consumer, 1, 1).unwrap();
        outputs.prepare(source, 2).unwrap();
        assert!(outputs.get(source).is_none());
        assert_eq!(*outputs.get(consumer).unwrap(), 20);
        assert!(matches!(
            outputs.publish(source, 1, 2),
            Err(OutputStoreError::StaleInput)
        ));
        assert_eq!(
            outputs.state(source).unwrap().current.unwrap().version,
            first.version
        );
        outputs.publish(source, 2, 2).unwrap();
        assert_eq!(*outputs.get(source).unwrap(), 30);
        assert!(outputs.state(source).unwrap().current.unwrap().version > first.version);
        outputs.invalidate(source).unwrap();
        assert!(outputs.get(source).is_none());
        assert_eq!(*outputs.get(consumer).unwrap(), 20);
    }

    #[test]
    fn reads_hold_storage_borrows_without_mutating_records() {
        let outputs = outputs();
        let source = entity(0);
        outputs.prepare(source, 1).unwrap();
        outputs.publish(source, 1, 0).unwrap();
        let version = outputs.state(source).unwrap().current.unwrap().version;
        let first = outputs.get(source).unwrap();
        let second = outputs.get(source).unwrap();
        assert_eq!(*first, *second);
        assert!(outputs.storage.try_borrow_mut().is_err());
        assert_eq!(
            outputs.state(source).unwrap().current.unwrap().version,
            version
        );
        drop((first, second));
        outputs.invalidate(source);
        outputs.storage_mut()[0] = Some(11);
        assert_eq!(outputs.storage()[0], Some(11));
        assert!(outputs.get(source).is_none());
    }

    #[test]
    fn nested_batches_freeze_history_until_the_next_outer_batch() {
        let outputs = outputs();
        let source = entity(0);
        outputs.set_retained(source, true).unwrap();
        outputs.prepare(source, 1).unwrap();
        {
            let _batch = outputs.begin_batch();
            assert!(outputs.previous(source).is_none());
            outputs.publish(source, 1, 0).unwrap();
            assert!(outputs.previous(source).is_none());
        }
        {
            let _batch = outputs.begin_batch();
            assert_eq!(*outputs.previous(source).unwrap(), 10);
            outputs.prepare(source, 2).unwrap();
            outputs.publish(source, 2, 1).unwrap();
            assert_eq!(*outputs.get(source).unwrap(), 20);
            {
                let _nested = outputs.begin_batch();
                assert_eq!(*outputs.previous(source).unwrap(), 10);
                assert!(outputs.state(source).unwrap().published_in_batch);
            }
            assert_eq!(*outputs.previous(source).unwrap(), 10);
        }
        {
            let _batch = outputs.begin_batch();
            assert_eq!(*outputs.previous(source).unwrap(), 20);
            assert!(!outputs.state(source).unwrap().published_in_batch);
        }
    }

    #[test]
    fn failed_production_preserves_last_success_and_frozen_history() {
        let outputs = outputs();
        let source = entity(0);
        outputs.set_retained(source, true).unwrap();
        outputs.prepare(source, 1).unwrap();
        outputs.publish(source, 1, 0).unwrap();
        outputs.prepare(source, 2).unwrap();
        {
            let _batch = outputs.begin_batch();
            assert!(outputs.get(source).is_none());
            assert_eq!(*outputs.previous(source).unwrap(), 10);
            outputs.storage_mut()[1] = None;
            outputs.invalidate(source);
            assert_eq!(*outputs.previous(source).unwrap(), 10);
        }
        {
            let _batch = outputs.begin_batch();
            assert_eq!(*outputs.previous(source).unwrap(), 10);
            assert!(outputs.get(source).is_none());
        }
        let old = outputs.set_retained(source, false).unwrap();
        assert_eq!(old.previous.unwrap().record, 0);
        assert!(outputs.previous(source).is_none());
    }

    #[test]
    fn enabling_history_does_not_retain_a_reclaimed_slot() {
        let outputs = outputs();
        let source = entity(0);
        outputs.prepare(source, 1).unwrap();
        outputs.publish(source, 1, 0).unwrap();
        outputs.invalidate(source);
        outputs.storage_mut()[0] = None;
        outputs.set_retained(source, true).unwrap();
        let _batch = outputs.begin_batch();
        assert!(outputs.previous(source).is_none());
        assert!(outputs.state(source).unwrap().current.is_none());
    }

    #[test]
    fn reset_releases_payload_records_without_dropping_history_demand() {
        let outputs = outputs();
        let source = entity(0);
        outputs.set_retained(source, true).unwrap();
        outputs.prepare(source, 1).unwrap();
        outputs.publish(source, 1, 0).unwrap();
        let _batch = outputs.begin_batch();
        outputs.publish(source, 1, 1).unwrap();
        let previous = outputs.reset(source).unwrap();
        assert_eq!(previous.current.unwrap().record, 1);
        assert_eq!(previous.previous.unwrap().record, 0);
        let state = outputs.state(source).unwrap();
        assert!(state.retained);
        assert_eq!(state.input_version, 1);
        assert!(outputs.get(source).is_none());
        assert!(outputs.previous(source).is_none());
        assert!(state.current.is_none());
        assert!(state.previous.is_none());
        outputs.publish(source, 1, 2).unwrap();
        assert_eq!(*outputs.get(source).unwrap(), 30);
        assert!(outputs.previous(source).is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn reserved_reads_publications_and_history_are_allocation_free() {
        let outputs = outputs();
        let source = entity(0);
        outputs.set_retained(source, true).unwrap();
        outputs.prepare(source, 0).unwrap();
        outputs.publish(source, 0, 0).unwrap();
        let allocations = crate::test_support::tracked_allocations(|| {
            for version in 1..16 {
                let _batch = outputs.begin_batch();
                outputs.prepare(source, version).unwrap();
                assert!(outputs.get(source).is_none());
                assert!(outputs.previous(source).is_some());
                outputs
                    .publish(source, version, version as usize % 3)
                    .unwrap();
                assert!(outputs.get(source).is_some());
            }
            outputs.release(source);
            outputs.prepare(source, 17).unwrap();
            outputs.publish(source, 17, 0).unwrap();
        });
        assert_eq!(allocations, 0);
    }
}
