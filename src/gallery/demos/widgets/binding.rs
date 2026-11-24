use super::state::{FormProgress, FormSlider};
use crate::ui::property::prop;
use crate::ui::widgets::{ProgressBar, Slider};

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
