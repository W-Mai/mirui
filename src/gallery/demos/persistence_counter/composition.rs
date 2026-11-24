use crate::gallery::sample_style::{
    centered_label, centered_sample, primary_action, secondary_action, stepper_track,
};
use crate::prelude::*;
use crate::ui::widgets::{Button, Text};

#[compose]
pub fn build_widgets(count: model!(Signal<i32>)) {
    //~focus-start
    ui! {
        Column (
            style: centered_sample(),
            row_gap: 14
        ) {
            Text (
                text: ${ format_args!("Saved  {}", count.get()) },
                capacity: "Saved  ".len() + 11,
                style: centered_label(),
                max_width: 260,
                height: 56,
                font_size: 26,
                text_color: ColorToken::OnSurface
            )
            Row (style: stepper_track(), max_width: 188) {
                Button (
                    "-",
                    style: secondary_action(),
                    grow: 1.0
                ) on Tap { count.update(|n| *n -= 1); }
                Button (
                    "+",
                    style: primary_action(),
                    grow: 1.0
                ) on Tap { count.update(|n| *n += 1); }
            }
            Text (
                "Persists across reloads and restarts",
                style: centered_label(),
                max_width: 280,
                height: 34,
                text_color: ColorToken::OnSurfaceVariant
            )
        }
    };
    //~focus-end
}
