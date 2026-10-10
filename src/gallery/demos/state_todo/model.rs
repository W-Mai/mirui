#[derive(Default)]
#[crate::model]
pub(super) struct TodoItemModel {
    #[observe]
    pub(super) completed: bool,
}

#[crate::model]
impl TodoItemModel {
    pub(super) fn toggle(&mut self) {
        self.completed = !self.completed;
    }
}

#[crate::model]
pub struct TodoModel {
    pub(super) milk: crate::model!(owned TodoItemModel),
    pub(super) docs: crate::model!(owned TodoItemModel),
    pub(super) release: crate::model!(owned TodoItemModel),
}

impl Default for TodoModel {
    fn default() -> Self {
        Self {
            milk: TodoItemModel::default().into(),
            docs: TodoItemModel::default().into(),
            release: TodoItemModel::default().into(),
        }
    }
}

#[crate::model]
impl TodoModel {
    #[observe]
    pub(super) fn remaining(&self) -> i32 {
        3 - i32::from(self.milk.completed())
            - i32::from(self.docs.completed())
            - i32::from(self.release.completed())
    }
}
