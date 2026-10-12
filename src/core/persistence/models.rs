use alloc::{borrow::Cow, boxed::Box, vec::Vec};

use super::{
    FieldCodec, ModelCodec, PersistenceError, PersistenceRegistry, archive::Schema,
    value::PersistValue,
};
use crate::{
    core::{
        model::{self, Candidate, Model, PersistModel, SaveEndpoint},
        storage::Storage,
    },
    ecs::World,
};

#[cfg(all(test, feature = "std"))]
mod tests;

pub(super) trait ModelEndpoint {
    fn is_alive(&self) -> bool;
    fn mark_pending(&self);
    fn save(
        &self,
        world: &World,
        key: &str,
        storage: &mut dyn Storage,
        output: &mut Vec<u8>,
        force: bool,
    ) -> Result<(), PersistenceError>;
}

impl<C: ModelCodec> ModelEndpoint for SaveEndpoint<C>
where
    C::Model: Model<Persistence = Candidate>,
{
    fn is_alive(&self) -> bool {
        self.is_alive()
    }
    fn mark_pending(&self) {
        self.mark_pending();
    }
    fn save(
        &self,
        world: &World,
        key: &str,
        storage: &mut dyn Storage,
        output: &mut Vec<u8>,
        force: bool,
    ) -> Result<(), PersistenceError> {
        self.save(world, key, storage, output, force).map(|_| ())
    }
}

pub(super) struct ModelEntry {
    pub key: Cow<'static, str>,
    pub endpoint: Box<dyn ModelEndpoint>,
    protected: Option<PersistenceError>,
}

impl ModelEntry {
    pub fn save(
        &mut self,
        world: &World,
        storage: &mut dyn Storage,
        output: &mut Vec<u8>,
        force: bool,
    ) -> Result<(), PersistenceError> {
        if !force && let Some(error) = self.protected {
            return Err(error);
        }
        self.endpoint
            .save(world, &self.key, storage, output, force)?;
        self.protected = None;
        Ok(())
    }
}

impl PersistenceRegistry {
    pub(super) fn check_key(&self, key: &str) -> Result<(), PersistenceError> {
        if key.is_empty() {
            return Err(PersistenceError::InvalidKey);
        }
        if self.items.iter().any(|item| item.key == key)
            || self
                .models
                .iter()
                .any(|item| item.key == key && item.endpoint.is_alive())
        {
            return Err(PersistenceError::DuplicateKey);
        }
        Ok(())
    }

    pub(crate) fn register_model<M>(
        &mut self,
        world: &mut World,
        value: M,
        key: Cow<'static, str>,
    ) -> M::Handle
    where
        M: PersistModel + PersistValue + Model<Persistence = Candidate>,
    {
        if let Err(error) = self.check_key(&key) {
            return reject_registration(world, value, error);
        }
        if let Some(codec) = self.codecs.find::<M>() {
            return codec.register(self, world, value, key);
        }
        if !matches!(M::SCHEMA, Schema::Record(fields) if !fields.is_empty()) {
            return reject_registration(world, value, PersistenceError::MissingCodec);
        }
        self.register_with_codec(world, value, key, FieldCodec::<M>::default())
    }

    pub(super) fn register_with_codec<M, C>(
        &mut self,
        world: &mut World,
        mut value: M,
        key: Cow<'static, str>,
        codec: C,
    ) -> M::Handle
    where
        M: Model<Persistence = Candidate>,
        C: ModelCodec<Model = M>,
    {
        let restored = if self.enabled {
            self.storage
                .read(&key)
                .map_err(Into::into)
                .and_then(|bytes| {
                    let Some(bytes) = bytes else {
                        return Ok(false);
                    };
                    let prepared = codec.prepare(&value, &bytes)?;
                    codec.apply(&mut value, prepared);
                    Ok(true)
                })
        } else {
            Ok(false)
        };
        let protected = restored.as_ref().err().copied();
        if let Some(error) = protected {
            crate::warn!(target: "mirui::persistence", "model persistence restore failed: {error}");
        }
        let (registration, handle) = model::register(world, value);
        let endpoint = registration.save_endpoint(codec, restored == Ok(true));
        self.models.retain(|item| item.endpoint.is_alive());
        self.models.push(ModelEntry {
            key,
            endpoint: Box::new(endpoint),
            protected,
        });
        let entity = world.spawn_empty();
        world.insert(entity, registration);
        handle
    }
}

fn reject_registration<M: Model<Persistence = Candidate>>(
    world: &mut World,
    value: M,
    error: PersistenceError,
) -> M::Handle {
    crate::warn!(target: "mirui::persistence", "model persistence registration failed: {error}");
    let (registration, handle) = model::register(world, value);
    registration.reject_persistence(error);
    let entity = world.spawn_empty();
    world.insert(entity, registration);
    handle
}
