use crate::prelude::*;
use crate::ui::widgets::Text;

pub(super) const ROW_H: i32 = 38;
pub(super) const POOL_SIZE: usize = 12;
pub(super) const ITEM_COUNT: u32 = 1 << 16;

pub(super) fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    let Some(label_entity) = world
        .get::<crate::ui::Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(t) = world.get_mut::<Text>(label_entity) {
        t.set_content(label);
        world.invalidate_visual(label_entity);
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.set_bg_color(if index % 2 == 0 {
            ColorToken::Surface
        } else {
            ColorToken::SurfaceVariant
        });
        world.invalidate_visual(entity);
    }
}
