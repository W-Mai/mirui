use super::{Entry, ModelList, Slot};
use crate::core::model::{CollectionError, CollectionLoadError, Model, collection::collect_input};

pub(in crate::core::model) struct PreparedLoad<M: Model> {
    replacement: ModelList<M>,
    len: usize,
}

impl<M: Model> ModelList<M> {
    /// Install a dataset, retiring every old item while retaining the collection's capacity and hard limit.
    ///
    /// Loading explicitly prepares nested member storage and may allocate. Preflight errors leave the old dataset unchanged and return the input. A successful load assigns new item identities, even when the values match.
    pub fn load<I: IntoIterator<Item = M>>(
        &mut self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        let (input, remaining) = collect_input(items.into_iter(), self.limit)?;
        let plan = match self.prepare_load(input.len()) {
            Ok(plan) => plan,
            Err(error) => return Err(CollectionLoadError::staged(error, input, remaining)),
        };
        plan.install(self, input.into_iter());
        Ok(())
    }

    pub(in crate::core::model) fn prepare_load(
        &self,
        len: usize,
    ) -> Result<PreparedLoad<M>, CollectionError> {
        let mut replacement = Self::new();
        replacement.limit = self.limit;
        let capacity = self.capacity().max(len);
        replacement.reserve(capacity)?;
        let binding = self.live_binding();
        for (index, slot) in replacement.slots.iter_mut().enumerate() {
            slot.generation = binding
                .as_ref()
                .and_then(|binding| binding.generation(index))
                .or_else(|| self.slots.get(index).map(|slot| slot.generation))
                .unwrap_or(0);
        }
        if replacement
            .slots
            .iter()
            .filter(|slot| slot.generation < u64::MAX)
            .count()
            < len
        {
            return Err(CollectionError::IdentityExhausted);
        }
        replacement.identity = self.identity;
        replacement.revision = self
            .revision
            .checked_add(1)
            .expect("model list revision exhausted");
        Ok(PreparedLoad { replacement, len })
    }
}

impl<M: Model> PreparedLoad<M> {
    pub(in crate::core::model) fn install(
        self,
        target: &mut ModelList<M>,
        input: impl ExactSizeIterator<Item = M>,
    ) {
        assert_eq!(input.len(), self.len, "prepared model load length");
        let mut replacement = self.replacement;
        let mut available = replacement
            .slots
            .iter_mut()
            .enumerate()
            .filter(|(_, slot)| slot.generation < u64::MAX);
        for (position, value) in input.enumerate() {
            let (index, slot) = available.next().expect("preflighted model slot");
            *slot = Slot {
                generation: slot.generation + 1,
                position: Some(position),
            };
            replacement.entries.push(Entry {
                slot: index,
                value: value.into(),
            });
        }
        if let Some(binding) = target.live_binding() {
            binding.ensure_binding(&mut replacement);
        }
        *target = replacement;
    }
}
