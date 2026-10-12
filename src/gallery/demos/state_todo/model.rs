use crate::core::model::{CollectionError, ModelList};
use alloc::string::String;

const LABELS: [&str; 6] = [
    "Buy milk",
    "Write docs",
    "Ship release",
    "Read a book",
    "Water plants",
    "Take a walk",
];

#[crate::model(persistence)]
#[derive(Default)]
pub(super) struct TodoItemModel {
    #[persist]
    pub(super) title: String,
    #[observe]
    #[persist]
    pub(super) completed: bool,
}

#[crate::model]
impl TodoItemModel {
    pub(super) fn toggle(&mut self) {
        self.completed = !self.completed;
    }
}

#[crate::model(persistence(key = "state_todo"))]
pub struct TodoModel {
    #[persist]
    pub(super) items: crate::model!(owned ModelList<TodoItemModel>),
    #[persist]
    next_label: usize,
}

impl Default for TodoModel {
    fn default() -> Self {
        let mut items = ModelList::bounded(LABELS.len());
        for title in &LABELS[..3] {
            items
                .try_push(TodoItemModel {
                    title: (*title).into(),
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
        let next_label = self.next_label % LABELS.len();
        self.items
            .try_push(TodoItemModel {
                title: LABELS[next_label].into(),
                completed: false,
            })
            .map_err(|(error, _)| error)?;
        self.next_label = (next_label + 1) % LABELS.len();
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
