use super::{ModelSet, ModelSetOwned};
use crate::core::{
    model::OwnedModel,
    persistence::{
        PersistenceError,
        archive::{Schema, Value, Writer},
        value::PersistValue,
    },
};
use alloc::vec::Vec;

impl<K: Ord + Default + PersistValue + 'static> PersistValue for ModelSet<K> {
    const SCHEMA: Schema = Schema::Set(&K::SCHEMA);
    type Prepared = Vec<K>;

    fn encode(&self, writer: &mut Writer<'_>) -> Result<(), PersistenceError> {
        writer.count(self.len())?;
        for value in &self.values {
            writer.sized(|writer| value.encode(writer))?;
        }
        Ok(())
    }

    fn prepare(&self, value: Value<'_>) -> Result<Self::Prepared, PersistenceError> {
        if !value.matches(&Self::SCHEMA) {
            return Err(PersistenceError::InvalidArchive);
        }
        let values = value.items()?;
        if self.limit.is_some_and(|limit| values.len() > limit) || self.version.1 == u64::MAX {
            return Err(PersistenceError::Capacity);
        }
        let mut input = Vec::new();
        input
            .try_reserve_exact(self.prepared.max(values.len()))
            .map_err(|_| PersistenceError::Capacity)?;
        for value in values {
            let mut item = K::default();
            item.apply(item.prepare(value)?);
            input.push(item);
        }
        input.sort_unstable();
        if input.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(PersistenceError::InvalidArchive);
        }
        Ok(input)
    }

    fn apply(&mut self, prepared: Self::Prepared) {
        self.install(prepared);
    }
}

impl<K: Ord + Default + PersistValue + 'static> PersistValue for ModelSetOwned<K> {
    const SCHEMA: Schema = ModelSet::<K>::SCHEMA;
    type Prepared = <ModelSet<K> as PersistValue>::Prepared;
    fn encode(&self, writer: &mut Writer<'_>) -> Result<(), PersistenceError> {
        self.storage().value.encode(writer)
    }
    fn prepare(&self, value: Value<'_>) -> Result<Self::Prepared, PersistenceError> {
        self.storage().value.prepare(value)
    }
    fn apply(&mut self, prepared: Self::Prepared) {
        self.storage_mut().apply_persisted(prepared);
    }
}
