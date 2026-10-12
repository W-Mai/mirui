use super::ModelSet;
use crate::core::model::{CollectionError, CollectionLoadError, collection::collect_input_checked};
use alloc::vec::Vec;
use core::cmp::Ordering;

impl<K: Ord + 'static> ModelSet<K> {
    /// Install owned values, removing duplicates and retaining prepared capacity and the hard limit.
    ///
    /// The limit counts distinct values, not input records. Failure preserves all input, including duplicates and the unconsumed iterator. Explicit loading may allocate staging memory proportional to the consumed input.
    pub fn load<I: IntoIterator<Item = K>>(
        &mut self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        let input = self.prepare_input(items.into_iter())?;
        self.install(input);
        Ok(())
    }

    /// Update membership, retaining existing values that compare equal to incoming values.
    ///
    /// Equivalent membership does not publish a change. Input order and duplicates do not affect the result. Preparation may allocate; errors preserve the old dataset and return all input.
    pub fn reconcile<I: IntoIterator<Item = K>>(
        &mut self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        let mut input = self.prepare_input(items.into_iter())?;
        if self.values == input {
            return Ok(());
        }
        let mut old = self.values.iter_mut().peekable();
        for value in &mut input {
            while old
                .peek()
                .is_some_and(|existing| (**existing).cmp(value) == Ordering::Less)
            {
                old.next();
            }
            if old
                .peek()
                .is_some_and(|existing| (**existing).cmp(value) == Ordering::Equal)
            {
                core::mem::swap(old.next().expect("matching set member"), value);
            }
        }
        self.install(input);
        Ok(())
    }

    fn prepare_input<I: Iterator<Item = K>>(
        &self,
        items: I,
    ) -> Result<Vec<K>, CollectionLoadError<I>> {
        let mut unique: Vec<usize> = Vec::new();
        let (mut input, remaining) = collect_input_checked(items, None, |input, item| {
            if let Err(position) = unique.binary_search_by(|&index| input[index].cmp(item)) {
                if self.limit.is_some_and(|limit| unique.len() == limit) {
                    return Err(CollectionError::Limit);
                }
                unique
                    .try_reserve(1)
                    .map_err(|_| CollectionError::Allocation)?;
                unique.insert(position, input.len());
            }
            Ok(())
        })?;
        drop(unique);
        if input
            .try_reserve_exact(self.prepared.saturating_sub(input.len()))
            .is_err()
        {
            return Err(CollectionLoadError::staged(
                CollectionError::Allocation,
                input,
                remaining,
            ));
        }
        input.sort_unstable();
        input.dedup();
        Ok(input)
    }

    pub(super) fn install(&mut self, input: Vec<K>) {
        self.prepared = self.prepared.max(input.len());
        let old = core::mem::replace(&mut self.values, input);
        self.changed();
        drop(old);
    }
}
