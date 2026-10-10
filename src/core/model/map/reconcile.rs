use super::{KeyEntry, ModelMap};
use crate::core::model::{CollectionError, CollectionLoadError, Model, collection::collect_input};
use alloc::vec::Vec;

impl<K: Ord + 'static, M: Model> ModelMap<K, M> {
    /// Merge input by the Map's business keys, retaining matching Model identities and handlers.
    ///
    /// Duplicate keys reject the batch before merging. The closure receives raw Model data, not the index key; it chooses which fields to overwrite. Failed preflight returns all input in original order. Preparation may allocate.
    pub fn reconcile<I: IntoIterator<Item = (K, M)>>(
        &mut self,
        items: I,
        mut merge: impl FnMut(&mut M, M),
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        let (mut input, remaining) = collect_input(items.into_iter(), self.limit())?;
        let mut order = Vec::new();
        if order.try_reserve_exact(input.len()).is_err() {
            return Err(CollectionLoadError::staged(
                CollectionError::Allocation,
                input,
                remaining,
            ));
        }
        order.extend(0..input.len());
        order.sort_unstable_by(|&a, &b| input[a].0.cmp(&input[b].0));
        if order
            .windows(2)
            .any(|pair| input[pair[0]].0 == input[pair[1]].0)
        {
            return Err(CollectionLoadError::staged(
                CollectionError::DuplicateKey,
                input,
                remaining,
            ));
        }
        let plan = match self.members.plan_matches(order.iter().map(|&index| {
            self.position(&input[index].0)
                .ok()
                .map(|index| self.keys[index].slot)
        })) {
            Ok(plan) => plan,
            Err(error) => return Err(CollectionLoadError::staged(error, input, remaining)),
        };
        if let Err(error) = self.reserve(plan.capacity - self.len()) {
            return Err(CollectionLoadError::staged(error, input, remaining));
        }
        drop(order);
        input.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let mut index = 0;
        self.retain(|_, _| {
            let keep = plan.keep[index];
            index += 1;
            keep
        });
        for (index, ((key, value), slot)) in input.into_iter().zip(plan.slots).enumerate() {
            let existing = self.members.contains_slot(slot);
            let id = self.members.merge_loaded(slot, value, &mut merge);
            if !existing {
                self.members
                    .move_to(id, index)
                    .expect("preflighted map order");
                self.keys.insert(index, KeyEntry { key, slot });
            }
        }
        Ok(())
    }
}
