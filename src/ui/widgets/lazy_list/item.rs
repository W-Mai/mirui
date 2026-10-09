use crate::core::model::{BindType, RegisteredModel};

/// The current data index of one retained list cell.
///
/// LazyList owns the instance. Read `index()` inside property bindings and event callbacks so reused cells follow their current item.
#[crate::model]
pub struct ListItem {
    index: u32,
}

#[crate::model]
impl ListItem {
    #[observe]
    pub fn index(&self) -> u32 {
        self.index
    }

    pub(crate) fn set_index(&mut self, index: u32) {
        self.index = index;
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
    pub fn new(world: &mut crate::ecs::World, list: crate::ecs::Entity, index: u32) -> Self {
        let (registration, handle) = crate::core::model::register(world, ListItem { index });
        Self {
            list,
            handle,
            _registration: registration,
        }
    }
}
