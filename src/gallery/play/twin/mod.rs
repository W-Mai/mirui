mod chapters;
mod model;
mod rules;
mod solver;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use chapters::{CHAPTER_MECHANICS, CHAPTER_NAMES, CHAPTER_TIPS};
#[cfg(any(feature = "persistence", test))]
#[allow(unused_imports)]
pub(crate) use model::SAVE_LEN;
pub(crate) use model::TwinModel;
pub(crate) use rules::{mapped, pressed_for};
pub(crate) use types::{TwinBoard, TwinMessage};
