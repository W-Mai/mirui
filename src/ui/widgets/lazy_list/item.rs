use crate::core::model::{BindType, ModelItemId, RegisteredModel};

/// The current index and optional Model identity of one retained list cell.
///
/// LazyList owns the instance. Read `index()` and `id()` inside property bindings and event callbacks so reused cells follow their current item. Generated rows have no Model ID.
#[crate::model]
pub struct ListItem {
    index: u32,
    id: Option<ModelItemId>,
}

#[crate::model]
impl ListItem {
    #[observe]
    pub fn index(&self) -> u32 {
        self.index
    }

    #[observe]
    pub fn id(&self) -> Option<ModelItemId> {
        self.id
    }

    pub(crate) fn rebind(&mut self, index: u32, id: Option<ModelItemId>) {
        self.index = index;
        self.id = id;
    }
}

pub(super) type ItemHandle = <ListItem as BindType>::Shared;

pub(super) struct CellItem {
    pub list: crate::ecs::Entity,
    pub handle: ItemHandle,
    _registration: RegisteredModel<ListItem>,
}

impl crate::ecs::Component for CellItem {}

impl CellItem {
    pub fn new(
        world: &mut crate::ecs::World,
        list: crate::ecs::Entity,
        index: u32,
        id: Option<ModelItemId>,
    ) -> Self {
        let (registration, handle) = crate::core::model::register(world, ListItem { index, id });
        Self {
            list,
            handle,
            _registration: registration,
        }
    }
}
