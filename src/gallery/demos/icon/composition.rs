use super::catalog::icons;
use super::motion::{beat, bounce, breathe};
use crate::prelude::*;
use crate::ui;
use crate::ui::icons::{ICON_CIRCLE, ICON_HEART, ICON_PLAY, ICON_PLUS, ICON_STAR};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::widgets::icon::Icon;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let table = icons();
    ui! {
        Column (
            id: "icon_gallery_shell",
            grow: 1.0,
            padding: Padding::all(12),
            row_gap: 8,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "VECTOR ICONS",
                height: 24,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "STATIC PATHS · LIVE THEME · RETAINED MOTION",
                height: 18,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "icon_gallery_grid",
                grow: 1.0,
                wrap: FlexWrap::Wrap,
                justify: JustifyContent::Center,
                align: AlignItems::Center,
                row_gap: 6,
                column_gap: 6
            ) {
                walk table.iter() with cell {
                    Image (
                        src: cell.0,
                        color: ThemedColor::Token(cell.1),
                        viewbox: Fixed::from_int(24),
                        scale: Fixed::from_ratio(7, 6),
                        width: 44,
                        height: 44,
                        grow: 0.0,
                        bg_color: ColorToken::SurfaceVariant,
                        border_radius: 10
                    )
                }
            }
            Text (
                "MOTION PRESETS",
                height: 18,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "icon_gallery_motion",
                height: 54,
                justify: JustifyContent::SpaceEvenly,
                align: AlignItems::Center,
                column_gap: 4
            ) {
                Icon (
                    path: ICON_HEART,
                    color: ThemedColor::Token(ColorToken::Error),
                    size: Dimension::Px(Fixed::from_int(34)),
                    grow: 1.0,
                    height: 52
                ) [
                    beat(),
                ]
                Icon (
                    path: ICON_CIRCLE,
                    color: ThemedColor::Token(ColorToken::Primary),
                    size: Dimension::Px(Fixed::from_int(34)),
                    grow: 1.0,
                    height: 52
                ) [
                    breathe(),
                ]
                Icon (
                    path: ICON_STAR,
                    color: ThemedColor::Token(ColorToken::Success),
                    size: Dimension::Px(Fixed::from_int(34)),
                    grow: 1.0,
                    height: 52
                ) [
                    bounce(),
                ]
                Icon (
                    path: ICON_PLAY,
                    color: ThemedColor::Token(ColorToken::Primary),
                    size: Dimension::Px(Fixed::from_int(34)),
                    grow: 1.0,
                    height: 52
                ) [
                    beat(),
                ]
                Icon (
                    path: ICON_PLUS,
                    color: ThemedColor::Token(ColorToken::OnSurface),
                    size: Dimension::Px(Fixed::from_int(34)),
                    grow: 1.0,
                    height: 52
                ) [
                    bounce(),
                ]
            }
        }
    };
}
