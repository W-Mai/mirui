use alloc::format;

use super::state::{FormProgress, FormSlider};
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::property::prop;
use crate::ui::widgets::lazy_list::BindItem;
use crate::ui::widgets::{ProgressBar, Slider, Text};

#[event]
pub(super) fn row_binder(
    event: event!(BindItem),
    rows: query!(Children),
    labels: query!(mut Text),
) {
    let label = format!("Row {}", event.index);
    let Some(label_entity) = rows
        .get(event.item)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = labels.get(label_entity) {
        text.set_content(label);
    }
}

#[mirui_macros::system]
pub fn slider_to_progress_system(
    sliders: query!(FormSlider, Slider),
    progress_bars: query!(Entity, ProgressBar; with FormProgress;),
    commands: commands!(),
) {
    let value = sliders
        .into_iter()
        .next()
        .map(|(_, slider)| slider.value.to_f32() / 100.0);
    let Some(v) = value else { return };
    for (entity, pb) in progress_bars {
        if (pb.value - v).abs() > 0.001 {
            commands.set::<prop::ProgressValue>(entity, v);
        }
    }
}
