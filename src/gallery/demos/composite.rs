extern crate alloc;

use crate::prelude::*;
use crate::render::command::CompositeMode;
use crate::ui::widgets::{Image, ParagraphStyle, Text};

const MODES: &[(&str, CompositeMode)] = &[
    ("source-over", CompositeMode::SourceOver),
    ("add", CompositeMode::Add),
    ("screen", CompositeMode::Screen),
    ("multiply", CompositeMode::Multiply),
    ("darken", CompositeMode::Darken),
    ("lighten", CompositeMode::Lighten),
    ("difference", CompositeMode::Difference),
];

const CELL_W: i32 = 82;
const CELL_H: i32 = 82;
const FG: i32 = 60;

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            bg_color: ColorToken::Surface,
            padding: Padding::all(12),
            align: AlignItems::Center
        ) {
            Text (
                "Composite modes",
                width: Dimension::percent(100),
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            Text (
                "Each tile blends the same image over red, green, blue, and black bands.",
                width: Dimension::percent(100),
                height: 38,
                text_color: ColorToken::OnSurfaceVariant
            )
            Row (
                width: Dimension::percent(100),
                justify: JustifyContent::Center,
                align: AlignItems::FlexStart,
                wrap: FlexWrap::Wrap,
                grow: 1.0,
                row_gap: 12,
                column_gap: 10
            ) {
                walk MODES.iter() with cell {
                    Column (
                        width: CELL_W,
                        height: CELL_H + 24,
                        align: AlignItems::Center,
                        row_gap: 3
                    ) {
                        View (
                            width: CELL_W,
                            height: CELL_H,
                            border_radius: 4,
                            direction: FlexDirection::Row
                        ) {
                            View (
                                bg_color: Color::rgb(220, 40, 40),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(40, 200, 80),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(60, 110, 230),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(20, 20, 20),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            Image (
                                position: Position::Absolute,
                                left: (CELL_W - FG) / 2,
                                top: (CELL_H - FG) / 2,
                                width: FG,
                                height: FG,
                                src: "thumbs_up",
                                composite: cell.1
                            )
                        }
                        Text (
                            cell.0,
                            width: CELL_W,
                            height: 18,
                            font_size: 10,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    }
                }
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: crate::surface::Surface,
    F: crate::app::RendererFactory<B>,
{
    use crate::app::plugins::{ImageResourcesPlugin, StdInstantClockPlugin};
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}
