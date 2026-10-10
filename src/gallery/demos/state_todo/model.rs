use crate::core::model::{CollectionError, ModelList};

const LABELS: [&str; 6] = [
    "Buy milk",
    "Write docs",
    "Ship release",
    "Read a book",
    "Water plants",
    "Take a walk",
];

#[crate::model]
pub(super) struct TodoItemModel {
    #[observe]
    pub(super) title: &'static str,
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
    pub(super) items: crate::model!(owned ModelList<TodoItemModel>),
    next_label: usize,
}

impl Default for TodoModel {
    fn default() -> Self {
        let mut items = ModelList::bounded(LABELS.len());
        for title in &LABELS[..3] {
            items
                .try_push(TodoItemModel {
                    title,
                    completed: false,
                })
                .unwrap_or_else(|_| unreachable!("initial tasks fit prepared capacity"));
        }
        Self {
            items: items.into(),
            next_label: 3,
        }
    }
}

#[crate::model]
impl TodoModel {
    #[observe]
    pub(super) fn remaining(&self) -> usize {
        self.items.iter().filter(|item| !item.completed()).count()
    }

    pub(super) fn add(&mut self) -> Result<(), CollectionError> {
        self.items
            .try_push(TodoItemModel {
                title: LABELS[self.next_label],
                completed: false,
            })
            .map_err(|(error, _)| error)?;
        self.next_label = (self.next_label + 1) % LABELS.len();
        Ok(())
    }

    pub(super) fn clear_completed(&mut self) {
        self.items.retain(|item| !item.completed());
    }

    pub(super) fn rotate(&mut self) {
        if self.items.len() > 1 {
            self.items.rotate_left(1).expect("valid task rotation");
        }
    }
}
