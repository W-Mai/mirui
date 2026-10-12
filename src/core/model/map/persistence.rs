use super::{KeyEntry, ModelMap, ModelMapOwned};
use crate::core::{
    model::{Model, OwnedModel, list::StagedLoad},
    persistence::{
        PersistenceError,
        archive::{Schema, Value, Writer},
        value::PersistValue,
    },
};
use alloc::vec::Vec;

#[doc(hidden)]
pub struct PreparedMap<K, M: Model> {
    members: StagedLoad<M>,
    keys: Vec<KeyEntry<K>>,
}

impl<K: Ord + Default + PersistValue + 'static, M: Model + Default + PersistValue> PersistValue
    for ModelMap<K, M>
{
    const SCHEMA: Schema = Schema::Map {
        key: &K::SCHEMA,
        value: &M::SCHEMA,
    };
    type Prepared = PreparedMap<K, M>;

    fn encode(&self, writer: &mut Writer<'_>) -> Result<(), PersistenceError> {
        writer.count(self.len())?;
        for (key, item) in self.iter() {
            writer.sized(|writer| key.encode(writer))?;
            writer.sized(|writer| item.storage().value.encode(writer))?;
        }
        Ok(())
    }

    fn prepare(&self, value: Value<'_>) -> Result<Self::Prepared, PersistenceError> {
        if !value.matches(&Self::SCHEMA) {
            return Err(PersistenceError::InvalidArchive);
        }
        let pairs = value.pairs()?;
        let plan = self
            .members
            .prepare_load(pairs.len())
            .map_err(|_| PersistenceError::Capacity)?;
        let mut input = Vec::new();
        let mut keys = Vec::new();
        input
            .try_reserve_exact(pairs.len())
            .map_err(|_| PersistenceError::Capacity)?;
        keys.try_reserve_exact(self.capacity().max(pairs.len()))
            .map_err(|_| PersistenceError::Capacity)?;
        for (key, value) in pairs {
            let mut item_key = K::default();
            item_key.apply(item_key.prepare(key)?);
            let mut item = M::default();
            item.apply(item.prepare(value)?);
            input.push((item_key, item));
        }
        input.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        if input.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(PersistenceError::InvalidArchive);
        }
        let members = plan.stage(input.into_iter().map(|(key, item)| {
            keys.push(KeyEntry { key, slot: 0 });
            item
        }));
        Ok(PreparedMap { members, keys })
    }

    fn apply(&mut self, mut prepared: Self::Prepared) {
        prepared.members.install(&mut self.members);
        for (entry, (id, _)) in prepared.keys.iter_mut().zip(self.members.entries()) {
            entry.slot = id.slot;
        }
        self.keys = prepared.keys;
    }
}

impl<K: Ord + Default + PersistValue + 'static, M: Model + Default + PersistValue> PersistValue
    for ModelMapOwned<K, M>
{
    const SCHEMA: Schema = ModelMap::<K, M>::SCHEMA;
    type Prepared = <ModelMap<K, M> as PersistValue>::Prepared;
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
