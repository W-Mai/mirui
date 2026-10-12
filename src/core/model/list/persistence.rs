use super::{ModelList, ModelListOwned, load::StagedLoad};
use crate::core::{
    model::{Model, OwnedModel},
    persistence::{
        PersistenceError,
        archive::{Schema, Value, Writer},
        value::PersistValue,
    },
};
use alloc::vec::Vec;

impl<M: Model + PersistValue + Default> PersistValue for ModelList<M> {
    const SCHEMA: Schema = Schema::Sequence(&M::SCHEMA);
    type Prepared = StagedLoad<M>;

    fn encode(&self, writer: &mut Writer<'_>) -> Result<(), PersistenceError> {
        writer.count(self.len())?;
        for item in self.iter() {
            writer.sized(|writer| item.storage().value.encode(writer))?;
        }
        Ok(())
    }

    fn prepare(&self, value: Value<'_>) -> Result<Self::Prepared, PersistenceError> {
        if !value.matches(&Self::SCHEMA) {
            return Err(PersistenceError::InvalidArchive);
        }
        let values = value.items()?;
        let plan = self
            .prepare_load(values.len())
            .map_err(|_| PersistenceError::Capacity)?;
        let mut input = Vec::new();
        input
            .try_reserve_exact(values.len())
            .map_err(|_| PersistenceError::Capacity)?;
        for value in values {
            let mut item = M::default();
            item.apply(item.prepare(value)?);
            input.push(item);
        }
        Ok(plan.stage(input.into_iter()))
    }

    fn apply(&mut self, prepared: Self::Prepared) {
        prepared.install(self);
    }
}

impl<M: Model + PersistValue + Default> PersistValue for ModelListOwned<M> {
    const SCHEMA: Schema = ModelList::<M>::SCHEMA;
    type Prepared = <ModelList<M> as PersistValue>::Prepared;
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
