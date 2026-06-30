extern crate alloc;

use crate::prelude::*;
use crate::render::command::CompositeMode;
use crate::ui::widgets::Image;

const MODES: &[(&str, &str, CompositeMode)] = &[
    ("source-over", "opaque overlay", CompositeMode::SourceOver),
    ("add", "lighten / glow", CompositeMode::Add),
    ("screen", "soft glow", CompositeMode::Screen),
    ("multiply", "tint / shadow", CompositeMode::Multiply),
    ("darken", "keep darker", CompositeMode::Darken),
    ("lighten", "keep brighter", CompositeMode::Lighten),
    ("difference", "invert / FX", CompositeMode::Difference),
];

const CELL_W: i32 = 96;
const CELL_H: i32 = 96;
const FG: i32 = 72;

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            bg_color: ColorToken::Surface,
            padding: Padding::all(12),
            align: AlignItems::Center
        ) {
            View (
                text: "Composite modes — yellow thumbs-up over an R/G/B/black stripe backdrop. Each mode rewrites the overlap differently.",
                height: 36
            )
            Row (
                justify: JustifyContent::SpaceBetween,
                align: AlignItems::FlexStart,
                grow: 1.0
            ) {
                walk MODES.iter() with cell {
                    Column (align: AlignItems::Center) {
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
                                composite: cell.2
                            )
                        }
                        View (text: cell.0, height: 18)
                        View (text: cell.1, height: 16)
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
    build_widgets(&mut app.world, parent);
}
