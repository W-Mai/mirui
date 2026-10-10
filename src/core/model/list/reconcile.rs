use super::{Entry, ModelList};
use crate::core::model::{
    CollectionError, CollectionLoadError, Model, OwnedModel, collection::collect_input,
};
use alloc::vec::Vec;

pub(in crate::core::model) struct MatchPlan {
    pub(in crate::core::model) slots: Vec<usize>,
    pub(in crate::core::model) keep: Vec<bool>,
    pub(in crate::core::model) capacity: usize,
}

impl<M: Model> ModelList<M> {
    /// Merge owned input by a frozen business key, retaining matching item identities.
    ///
    /// Incoming order becomes list order. Duplicate keys in either dataset reject the batch before `merge` runs. The closure receives raw Model data and decides which fields to replace; it does not return a rollback result. Preparation may allocate, including storage for new nested shapes.
    pub fn reconcile<I, K>(
        &mut self,
        items: I,
        mut key_of: impl FnMut(&M) -> K,
        mut merge: impl FnMut(&mut M, M),
    ) -> Result<(), CollectionLoadError<I::IntoIter>>
    where
        I: IntoIterator<Item = M>,
        K: Ord,
    {
        let (input, remaining) = collect_input(items.into_iter(), self.limit)?;
        let plan = match self.match_input(&input, &mut key_of) {
            Ok(plan) => plan,
            Err(error) => return Err(CollectionLoadError::staged(error, input, remaining)),
        };
        if let Err(error) = self.reserve(plan.capacity - self.len()) {
            return Err(CollectionLoadError::staged(error, input, remaining));
        }
        let mut index = 0;
        self.retain(|_| {
            let keep = plan.keep[index];
            index += 1;
            keep
        });
        for (index, (value, slot)) in input.into_iter().zip(plan.slots).enumerate() {
            let id = self.merge_loaded(slot, value, &mut merge);
            let position = self.position(id).expect("merged model slot");
            self.swap(index, position).expect("preflighted list order");
        }
        Ok(())
    }

    pub(in crate::core::model) fn merge_loaded(
        &mut self,
        slot: usize,
        value: M,
        merge: &mut impl FnMut(&mut M, M),
    ) -> super::ModelItemId {
        if let Some(position) = self.slots[slot].position {
            let storage = self.entries[position].value.storage_mut();
            merge(&mut storage.value, value);
            storage.pending.as_mut().fill(true);
        } else {
            let binding = self.live_binding();
            let mut value: M::Owned = value.into();
            if let Some(binding) = &binding {
                binding.activate_loaded(slot, value.storage_mut());
            }
            self.slots[slot].generation += 1;
            if let Some(binding) = &binding {
                binding.set_generation(slot, self.slots[slot].generation);
            }
            let position = self.len();
            self.entries.push(Entry { slot, value });
            self.slots[slot].position = Some(position);
            self.changed();
        }
        self.id(slot)
    }

    fn match_input<K: Ord>(
        &self,
        input: &[M],
        key_of: &mut impl FnMut(&M) -> K,
    ) -> Result<MatchPlan, CollectionError> {
        let mut old = Vec::new();
        let mut new = Vec::new();
        old.try_reserve_exact(self.len())
            .map_err(|_| CollectionError::Allocation)?;
        new.try_reserve_exact(input.len())
            .map_err(|_| CollectionError::Allocation)?;
        for entry in &self.entries {
            old.push((key_of(&entry.value.storage().value), entry.slot));
        }
        for (index, item) in input.iter().enumerate() {
            new.push((key_of(item), index));
        }
        old.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        new.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        if old.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || new.windows(2).any(|pair| pair[0].0 == pair[1].0)
        {
            return Err(CollectionError::DuplicateKey);
        }
        new.sort_unstable_by_key(|entry| entry.1);
        self.plan_matches(new.iter().map(|(key, _)| {
            old.binary_search_by(|entry| entry.0.cmp(key))
                .ok()
                .map(|index| old[index].1)
        }))
    }

    pub(in crate::core::model) fn plan_matches(
        &self,
        matches: impl ExactSizeIterator<Item = Option<usize>>,
    ) -> Result<MatchPlan, CollectionError> {
        let mut slots = Vec::new();
        let mut keep = Vec::new();
        let capacity = self.capacity().max(matches.len());
        slots
            .try_reserve_exact(matches.len())
            .map_err(|_| CollectionError::Allocation)?;
        keep.try_reserve_exact(self.len())
            .map_err(|_| CollectionError::Allocation)?;
        keep.resize(self.len(), false);
        for slot in matches {
            if let Some(slot) = slot {
                let position = self.slots[slot].position.expect("active matched slot");
                assert!(!keep[position], "duplicate matched model slot");
                keep[position] = true;
            }
            slots.push(slot.unwrap_or(usize::MAX));
        }
        let binding = self.live_binding();
        let mut free = (0..capacity).filter(|&index| {
            if self
                .slots
                .get(index)
                .and_then(|slot| slot.position)
                .is_some_and(|position| keep[position])
            {
                return false;
            }
            binding
                .as_ref()
                .and_then(|binding| binding.generation(index))
                .or_else(|| self.slots.get(index).map(|slot| slot.generation))
                .unwrap_or(0)
                < u64::MAX
        });
        for slot in &mut slots {
            if *slot == usize::MAX {
                *slot = free.next().ok_or(CollectionError::IdentityExhausted)?;
            }
        }
        Ok(MatchPlan {
            slots,
            keep,
            capacity,
        })
    }
}
