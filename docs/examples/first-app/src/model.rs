use mirui::core::model::{ModelItemId, ModelList};
use mirui::prelude::*;

pub const TITLES: [&str; 3] = ["Connect the display", "Open a window", "Try the browser"];
pub(crate) const TASK_LIMIT: usize = 8;

// #region task
//~focus-start:ch2,ch6
//~add-start:ch6
#[model(persistence)]
//~add-end:ch6
#[derive(Default)]
//~add-start:ch2
pub struct Task {
    //~add-start:ch6
    #[persist]
    //~add-end:ch6
    pub title: String,
    #[observe]
    //~add-start:ch6
    #[persist]
    //~add-end:ch6
    pub done: bool,
}

#[model]
impl Task {
    pub fn toggle(&mut self) {
        self.done = !self.done;
    }
}
//~add-end:ch2
//~focus-end:ch2,ch6
// #endregion task

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Open,
    Done,
}

impl Filter {
    pub fn includes(self, done: bool) -> bool {
        match self {
            Self::All => true,
            Self::Open => !done,
            Self::Done => done,
        }
    }
}

//~focus-start:ch6
//~add-start:ch6
#[model(persistence(key = "first_app_tasks"))]
pub struct Tasks {
    #[persist]
    pub items: model!(owned ModelList<Task>),
    #[observe]
    pub(crate) filter: Filter,
    #[persist]
    sequence: u32,
}
//~add-end:ch6
//~focus-end:ch6

impl Default for Tasks {
    fn default() -> Self {
        let mut items = ModelList::bounded(TASK_LIMIT);
        for (index, title) in TITLES.into_iter().enumerate() {
            assert!(
                items
                    .try_push(Task {
                        title: title.into(),
                        done: index == 1,
                    })
                    .is_ok()
            );
        }
        Self {
            items: items.into(),
            filter: Filter::All,
            sequence: 1,
        }
    }
}

#[model]
impl Tasks {
    // #region collection
    //~focus-start:ch2
    //~add-start:ch2
    #[observe]
    pub fn completed(&self) -> usize {
        self.items.iter().filter(|task| task.done()).count()
    }

    #[observe]
    pub fn total(&self) -> usize {
        self.items.len()
    }
    //~add-end:ch2
    //~focus-end:ch2

    //~focus-start:ch3
    //~add-start:ch3
    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
    }

    pub fn add(&mut self, title: Option<String>) {
        let title = title.unwrap_or_else(|| format!("Task {}", self.sequence));
        if self.items.try_push(Task { title, done: false }).is_ok() {
            self.sequence = self.sequence.saturating_add(1);
        }
    }

    pub fn remove(&mut self, key: ModelItemId) {
        let _ = self.items.remove(key);
    }
    //~add-end:ch3
    //~focus-end:ch3
    // #endregion collection
}
