use super::model::CounterModel;
use crate::gallery::sample_style::{
    centered_label, centered_sample, primary_action, secondary_action, stepper_track,
};
use crate::prelude::*;
use crate::ui::widgets::{Button, Text};

#[compose]
pub fn build_widgets(counter: model!(CounterModel)) {
    //~focus-start
    ui! {
        Column (
            style: centered_sample(),
            row_gap: 6,
            font_size: 12,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                "Count",
                style: centered_label(),
                height: 16,
                text_color: ColorToken::OnSurfaceVariant
            )
            Text (
                style: centered_label(),
                text: ${ format_args!("{}", counter.count()) },
                capacity: 11,
                height: 76,
                font_size: 48
            )
            Row (style: stepper_track(), max_width: 208) {
                Button (
                    "-",
                    style: secondary_action(),
                    grow: 1.0
                ) on Tap { counter.decrement(); }
                Button (
                    "+",
                    style: primary_action(),
                    grow: 1.0
                ) on Tap { counter.increment(); }
            }
        }
    };
    //~focus-end
}
