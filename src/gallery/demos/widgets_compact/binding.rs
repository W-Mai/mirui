use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;
use crate::ui::widgets::lazy_list::BindItem;

pub(super) const ROW_HEIGHT: i32 = 12;
pub(super) const POOL_SIZE: usize = 9;
pub(super) const VIRTUAL_ITEM_COUNT: u32 = 600_000;
pub(super) const VIRTUAL_CONTENT_HEIGHT: i32 = ROW_HEIGHT * VIRTUAL_ITEM_COUNT as i32;
const ROW_LABELS: [&str; 8] = [
    "ALPHA", "BETA", "GAMMA", "DELTA", "EPS", "ZETA", "ETA", "THETA",
];

#[event]
pub(super) fn bind_row(event: event!(BindItem), rows: query!(Children), labels: query!(mut Text)) {
    let Some(label) = rows
        .get(event.item)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = labels.get(label) {
        text.set_content(ROW_LABELS[event.index as usize % ROW_LABELS.len()]);
    }
}
