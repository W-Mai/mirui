use crate::prelude::{Entity, World};
use crate::ui::Children;
use crate::ui::widgets::Text;

pub(super) const ROW_HEIGHT: i32 = 12;
pub(super) const POOL_SIZE: usize = 9;
pub(super) const VIRTUAL_ITEM_COUNT: u32 = 600_000;
pub(super) const VIRTUAL_CONTENT_HEIGHT: i32 = ROW_HEIGHT * VIRTUAL_ITEM_COUNT as i32;
const ROW_LABELS: [&str; 8] = [
    "ALPHA", "BETA", "GAMMA", "DELTA", "EPS", "ZETA", "ETA", "THETA",
];

pub(super) fn bind_row(world: &mut World, entity: Entity, index: u32) {
    let Some(label) = world
        .get::<Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = world.get_mut::<Text>(label) {
        text.set_content(ROW_LABELS[index as usize % ROW_LABELS.len()]);
        world.invalidate_visual(label);
    }
}
