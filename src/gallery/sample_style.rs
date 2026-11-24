//! Shared styles for the Gallery's API samples.

use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

pub(super) fn centered_sample() -> StylePreset {
    style!(grow: 1.0, align: AlignItems::Center, justify: JustifyContent::Center, padding: Padding::all(20))
}

pub(super) fn centered_label() -> StylePreset<Text> {
    style!(width: Dimension::percent(100), paragraph: ParagraphStyle::label())
}

pub(super) fn primary_action() -> StylePreset<Button> {
    style!(
        height: 44, border_radius: 22, normal_color: ColorToken::Primary,
        pressed_color: ColorToken::Secondary, text_color: ColorToken::OnPrimary
    )
}

pub(super) fn secondary_action() -> StylePreset<Button> {
    style!(
        height: 44, border_radius: 22, normal_color: ColorToken::Outline,
        pressed_color: ColorToken::SurfaceVariant, text_color: ColorToken::OnSurface
    )
}

pub(super) fn stepper_track() -> StylePreset {
    style!(
        width: Dimension::percent(100), height: 52, padding: Padding::all(4), column_gap: 4,
        bg_color: ColorToken::SurfaceVariant, border_radius: 26, font_size: 26
    )
}
