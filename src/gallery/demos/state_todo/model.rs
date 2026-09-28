#[derive(Default)]
#[crate::model]
pub struct TodoModel {
    #[observe]
    pub(super) milk: bool,
    #[observe]
    pub(super) docs: bool,
    #[observe]
    pub(super) release: bool,
}

#[derive(Clone, Copy)]
pub(super) enum TodoItem {
    Milk,
    Docs,
    Release,
}

#[crate::model]
impl TodoModel {
    #[observe]
    pub(super) fn remaining(&self) -> i32 {
        3 - i32::from(self.milk) - i32::from(self.docs) - i32::from(self.release)
    }

    pub(super) fn toggle(&mut self, item: TodoItem) {
        match item {
            TodoItem::Milk => self.milk = !self.milk,
            TodoItem::Docs => self.docs = !self.docs,
            TodoItem::Release => self.release = !self.release,
        }
    }
}
