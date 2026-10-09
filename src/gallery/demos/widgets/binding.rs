use super::state::{FormProgress, FormSlider};
use crate::prelude::*;
use crate::ui::property::prop;
use crate::ui::widgets::lazy_list::CreateCell;
use crate::ui::widgets::{ParagraphStyle, ProgressBar, Slider, Text, TextAlign};

#[event]
pub(super) fn create_row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! {
        Row (
            bg_color: ColorToken::Surface,
            width: Dimension::percent(100),
            align: AlignItems::Center,
            padding: Padding {
                top: Dimension::px(0),
                right: Dimension::px(14),
                bottom: Dimension::px(0),
                left: Dimension::px(14),
            }
        ) {
            Text (
                text: ${ alloc::format!("Row {}", item.index()) },
                grow: 1.0,
                height: 24,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
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
