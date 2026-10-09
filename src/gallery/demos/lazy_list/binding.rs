use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;
use crate::ui::widgets::lazy_list::BindItem;

pub(super) const ROW_H: i32 = 38;
pub(super) const POOL_SIZE: usize = 12;
pub(super) const ITEM_COUNT: u32 = 1 << 16;

#[event]
pub(super) fn row_binder(
    event: event!(BindItem),
    rows: query!(Children),
    labels: query!(mut Text),
    styles: query!(mut Style),
) {
    let label = alloc::format!("Row {}", event.index);
    let Some(label_entity) = rows
        .get(event.item)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(t) = labels.get(label_entity) {
        t.set_content(label);
    }
    if let Some(style) = styles.get(event.item) {
        style.set_bg_color(if event.index % 2 == 0 {
            ColorToken::Surface
        } else {
            ColorToken::SurfaceVariant
        });
    }
}
