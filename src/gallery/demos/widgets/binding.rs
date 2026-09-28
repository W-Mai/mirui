use alloc::format;

use super::state::{FormProgress, FormSlider};
use crate::prelude::{Entity, World};
use crate::ui::Children;
use crate::ui::widgets::{ProgressBar, Slider, Text};

pub(super) fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = format!("Row {index}");
    let Some(label_entity) = world
        .get::<Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = world.get_mut::<Text>(label_entity) {
        text.set_content(label);
        world.invalidate_visual(label_entity);
    }
}

#[mirui_macros::system]
pub fn slider_to_progress_system(world: &mut World) {
    let value = world
        .query::<FormSlider>()
        .iter()
        .find_map(|(entity, _)| world.get::<Slider>(entity))
        .map(|slider| slider.value.to_f32() / 100.0);
    let Some(v) = value else { return };
    world.for_each_stable::<FormProgress>(|world, entity| {
        if let Some(pb) = world.get_mut::<ProgressBar>(entity)
            && (pb.value - v).abs() > 0.001
        {
            pb.value = v;
            world.invalidate(entity);
        }
    });
}
