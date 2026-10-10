use super::{KeyEntry, ModelMap};
use crate::core::model::{CollectionError, CollectionLoadError, Model, collection::collect_input};
use alloc::vec::Vec;

impl<K: Ord + 'static, M: Model> ModelMap<K, M> {
    /// Load key-value pairs, retiring old members and rejecting duplicate keys before installation.
    ///
    /// Failure returns all consumed and remaining input in original order. Successful installation sorts by business key and explicitly prepares member storage, including nested collection shapes.
    pub fn load<I: IntoIterator<Item = (K, M)>>(
        &mut self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        let (mut input, remaining) = collect_input(items.into_iter(), self.limit())?;
        if let Err(error) = unique_keys(&input) {
            return Err(CollectionLoadError::staged(error, input, remaining));
        }
        let plan = match self.members.prepare_load(input.len()) {
            Ok(plan) => plan,
            Err(error) => return Err(CollectionLoadError::staged(error, input, remaining)),
        };
        let mut keys = Vec::new();
        if keys
            .try_reserve_exact(self.capacity().max(input.len()))
            .is_err()
        {
            return Err(CollectionLoadError::staged(
                CollectionError::Allocation,
                input,
                remaining,
            ));
        }
        input.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        plan.install(
            &mut self.members,
            input.into_iter().map(|(key, value)| {
                keys.push(KeyEntry { key, slot: 0 });
                value
            }),
        );
        for (entry, (id, _)) in keys.iter_mut().zip(self.members.entries()) {
            entry.slot = id.slot;
        }
        self.keys = keys;
        Ok(())
    }
}

fn unique_keys<K: Ord, M>(input: &[(K, M)]) -> Result<(), CollectionError> {
    let mut keys = Vec::new();
    keys.try_reserve_exact(input.len())
        .map_err(|_| CollectionError::Allocation)?;
    keys.extend(input.iter().map(|(key, _)| key));
    keys.sort_unstable();
    if keys.windows(2).any(|pair| pair[0] == pair[1]) {
        Err(CollectionError::DuplicateKey)
    } else {
        Ok(())
    }
}
