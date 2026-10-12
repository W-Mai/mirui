use alloc::borrow::Cow;

use crate::{
    core::model::{Model, ModelRegistration, PersistModel},
    ecs::World,
};

/// A pending Model registration. Only `finish` installs the instance and restores persisted data.
#[must_use = "call .finish() to register the model"]
pub struct ModelBuilder<'a, M: Model> {
    world: &'a mut World,
    value: M,
    key: Option<Cow<'static, str>>,
}

impl<'a, M: Model> ModelBuilder<'a, M> {
    pub(super) fn new(world: &'a mut World, value: M) -> Self {
        Self {
            world,
            value,
            key: None,
        }
    }

    /// Register the instance, restore available persisted data, and return its shared handle.
    pub fn finish(self) -> M::Handle
    where
        M::Persistence: ModelRegistration<M>,
    {
        M::Persistence::register(self.world, self.value, self.key)
    }
}

impl<M: PersistModel> ModelBuilder<'_, M> {
    /// Override the type's default storage key before registration. Literal keys are borrowed; owned strings are moved.
    pub fn with_key(mut self, key: impl Into<Cow<'static, str>>) -> Self {
        self.key = Some(key.into());
        self
    }
}
