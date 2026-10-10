use alloc::rc::Rc;
use core::any::Any;

use crate::core::model::{
    BindingOwner, Model, ModelHandle, ModelInvalidation, ModelItemId, ModelListHandle,
    ModelMapHandle, ModelOwnerError, ModelSubscription,
};
use crate::ecs::{Entity, World};

/// A generated row count or a registered Model collection used by `LazyList`.
///
/// Numbers create indexed rows without allocating Models. List and Map instances supply observable membership and stable item identities. Cloning this value shares its source.
#[derive(Clone)]
pub struct ListItems(Source);

#[derive(Clone)]
enum Source {
    Count(u32),
    Collection(Rc<dyn CollectionSource>),
}

trait Collection: ModelHandle + 'static {
    fn count(&self) -> usize;
    fn item_id(&self, index: usize) -> Option<ModelItemId>;
    fn subscribe(&self, world: &World, entity: Entity) -> ModelSubscription;
}

impl<M: Model> Collection for ModelListHandle<M> {
    fn count(&self) -> usize {
        self.len()
    }
    fn item_id(&self, index: usize) -> Option<ModelItemId> {
        self.item_id(index)
    }
    fn subscribe(&self, world: &World, entity: Entity) -> ModelSubscription {
        self.subscribe_observed(0, world, entity, ModelInvalidation::Layout)
    }
}

impl<K: Ord + 'static, M: Model> Collection for ModelMapHandle<K, M> {
    fn count(&self) -> usize {
        self.len()
    }
    fn item_id(&self, index: usize) -> Option<ModelItemId> {
        self.members_handle().item_id(index)
    }
    fn subscribe(&self, world: &World, entity: Entity) -> ModelSubscription {
        Collection::subscribe(&self.members_handle(), world, entity)
    }
}

trait CollectionSource {
    fn as_any(&self) -> &dyn Any;
    fn same(&self, other: &dyn CollectionSource) -> bool;
    fn owner(&self) -> BindingOwner;
    fn len(&self) -> usize;
    fn item_id(&self, index: usize) -> Option<ModelItemId>;
    fn subscribe(&self, world: &World, entity: Entity) -> Option<ModelSubscription>;
}

impl<T: Collection> CollectionSource for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn CollectionSource) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.access().same_identity(other.access()))
    }
    fn owner(&self) -> BindingOwner {
        BindingOwner::registered(self)
    }
    fn len(&self) -> usize {
        if self.access().upgrade().is_some() {
            self.count()
        } else {
            0
        }
    }
    fn item_id(&self, index: usize) -> Option<ModelItemId> {
        self.access().upgrade()?;
        Collection::item_id(self, index)
    }
    fn subscribe(&self, world: &World, entity: Entity) -> Option<ModelSubscription> {
        self.access().upgrade()?;
        Some(Collection::subscribe(self, world, entity))
    }
}

#[derive(Debug)]
pub(super) enum SourceError {
    Owner(ModelOwnerError),
    IndexRange,
}

impl core::fmt::Display for SourceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Owner(error) => write!(f, "{error:?}"),
            Self::IndexRange => f.write_str("collection exceeds the row index range"),
        }
    }
}

impl Default for ListItems {
    fn default() -> Self {
        0_u32.into()
    }
}

impl From<u32> for ListItems {
    fn from(count: u32) -> Self {
        Self(Source::Count(count))
    }
}

impl<M: Model> From<ModelListHandle<M>> for ListItems {
    fn from(items: ModelListHandle<M>) -> Self {
        Self(Source::Collection(Rc::new(items)))
    }
}

impl<K: Ord + 'static, M: Model> From<ModelMapHandle<K, M>> for ListItems {
    fn from(items: ModelMapHandle<K, M>) -> Self {
        Self(Source::Collection(Rc::new(items)))
    }
}

impl PartialEq for ListItems {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Source::Count(left), Source::Count(right)) => left == right,
            _ => self.same_source(other),
        }
    }
}
impl Eq for ListItems {}

impl ListItems {
    /// Current row count; a retired collection has no readable rows.
    pub fn len(&self) -> usize {
        match &self.0 {
            Source::Count(count) => *count as usize,
            Source::Collection(source) => source.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn count(&self, world: &World) -> Result<u32, SourceError> {
        if let Source::Collection(source) = &self.0 {
            source
                .owner()
                .check(world.id())
                .map_err(SourceError::Owner)?;
        }
        u32::try_from(self.len()).map_err(|_| SourceError::IndexRange)
    }

    pub(super) fn id(&self, index: u32) -> Option<ModelItemId> {
        match &self.0 {
            Source::Count(_) => None,
            Source::Collection(source) => source.item_id(index as usize),
        }
    }

    pub(super) fn same_source(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Source::Count(_), Source::Count(_)) => true,
            (Source::Collection(left), Source::Collection(right)) => left.same(&**right),
            _ => false,
        }
    }

    pub(super) fn subscribe(&self, world: &World, entity: Entity) -> Option<ModelSubscription> {
        match &self.0 {
            Source::Count(_) => None,
            Source::Collection(source) => source.subscribe(world, entity),
        }
    }
}
