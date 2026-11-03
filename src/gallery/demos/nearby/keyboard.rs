use super::actions::ActionAnchor;
use super::style::*;
use super::widgets::{caption, shadow};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::TextAlign::Center;
use alloc::string::String;

#[compose]
pub(super) fn keycap(
    text: prop!(String),
    selected: prop!(bool),
    motion: prop!(Transform),
    anchor: ActionAnchor,
    size: u16,
    metrics: Metrics,
) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, transform: ${ motion }) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                justify: JustifyContent::Center,
                padding: Padding {
                    right: 12.into(),
                    ..Default::default()
                },
                border_width: 1,
                border_color: ${ if selected.get() { FOREGROUND } else { DIVIDER } },
                bg_color: ${ if selected.get() { ACCENT } else { PANEL } }
            ) [
                anchor,
            ] {
                caption (
                    text,
                    ${ if selected.get() { ON_ACCENT } else { FOREGROUND } },
                    size,
                    Center,
                    LABEL_LEADING
                )
            }
            shadow (metrics)
        }
    }
}
