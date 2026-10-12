use super::model::SettingsModel;
use crate::prelude::*;
use crate::ui::widgets::{Button, Checkbox, ParagraphStyle, Slider, Text};

#[compose]
pub fn build_widgets(settings: model!(SettingsModel)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Text ("READER SETTINGS", height: 30, font_size: 18)
            Text ("Text size", height: 22, font_size: 14)
            Slider (
                id: "settings_size",
                height: 30,
                min: Fixed::from_int(12),
                max: Fixed::from_int(24),
                value: ${ Fixed::from_int(i32::from(settings.preview_size())) }
            ) on ValueChanged { settings.set_font_size(event.new.to_int().clamp(12, 24) as u16); }
            Row (height: 30, align: AlignItems::Center, column_gap: 10) {
                Checkbox (
                    id: "settings_details",
                    width: 24,
                    height: 24,
                    checked: ${ settings.show_details() }
                ) on Toggled { settings.set_show_details(event.now); }
                Text ("Show details", grow: 1.0, font_size: 14)
            }
            Button (id: "settings_preview_toggle", height: 32, border_radius: 8) [
                Text::label("Toggle preview"),
            ] on Tap { settings.toggle_preview(); }
            Column (
                padding: Padding::all(10),
                row_gap: 6,
                bg_color: ColorToken::SurfaceVariant,
                visible: ${ settings.preview_open() }
            ) {
                Text (
                    "A quiet morning",
                    id: "settings_preview_title",
                    height: 32,
                    font_size: ${ settings.preview_size() },
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Notes from the window seat.",
                    id: "settings_preview_details",
                    height: 24,
                    font_size: 12,
                    visible: ${ settings.show_details() },
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    };
}
