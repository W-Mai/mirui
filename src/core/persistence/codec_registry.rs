use alloc::{borrow::Cow, boxed::Box, rc::Rc, vec::Vec};
use core::any::Any;

use super::{ModelCodec, PersistenceRegistry};
use crate::{
    core::model::{Candidate, Model, PersistModel},
    ecs::World,
};

#[derive(Default)]
pub(super) struct CodecRegistry {
    entries: Vec<Box<dyn Any>>,
}

pub(super) trait CodecRegistration<M: Model> {
    fn register(
        self: Rc<Self>,
        registry: &mut PersistenceRegistry,
        world: &mut World,
        value: M,
        key: Cow<'static, str>,
    ) -> M::Handle;
}

impl<M, C> CodecRegistration<M> for C
where
    M: PersistModel + Model<Persistence = Candidate>,
    C: ModelCodec<Model = M>,
{
    fn register(
        self: Rc<Self>,
        registry: &mut PersistenceRegistry,
        world: &mut World,
        value: M,
        key: Cow<'static, str>,
    ) -> M::Handle {
        registry.register_with_codec(world, value, key, self)
    }
}

impl CodecRegistry {
    pub fn insert<C: ModelCodec>(&mut self, codec: C)
    where
        C::Model: PersistModel + Model<Persistence = Candidate>,
    {
        if self.find::<C::Model>().is_some() {
            crate::warn!(target: "mirui::persistence", "a codec is already configured for this Model type");
            return;
        }
        let codec: Rc<dyn CodecRegistration<C::Model>> = Rc::new(codec);
        self.entries.push(Box::new(codec));
    }

    pub fn find<M: Model>(&self) -> Option<Rc<dyn CodecRegistration<M>>> {
        self.entries.iter().find_map(|entry| {
            entry
                .downcast_ref::<Rc<dyn CodecRegistration<M>>>()
                .cloned()
        })
    }
}
