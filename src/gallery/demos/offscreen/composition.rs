use super::state::{ForceDirty, FpsReadout, PanelTarget};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

const PANEL_W: i32 = 280;
const PANEL_H: i32 = 260;
const GRID_COLS: i32 = 6;
const GRID_ROWS: i32 = 9;
const TILE_W: i32 = 36;
const TILE_H: i32 = 22;
const TILE_GAP: i32 = 4;
const TILE_PAD: i32 = 8;

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

#[compose]
pub fn build_widgets() {
    let root = cx.parent();
    cx.world_mut().insert(root, ForceDirty);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            row_gap: 6,
            padding: Padding::all(12),
            bg_color: ColorToken::SurfaceVariant
        ) {
            View (
                id: "offscreen_panel",
                bg_color: ColorToken::Surface,
                border_radius: Fixed::from_int(12),
                width: PANEL_W,
                height: PANEL_H
            ) [
                PanelTarget,
            ] {
                walk 0..(GRID_COLS * GRID_ROWS) with i {
                    View (
                        bg_color: tile_color(i),
                        border_color: ColorToken::OnSurface,
                        border_width: Fixed::ONE,
                        border_radius: Fixed::from_int(6),
                        position: Position::Absolute,
                        left: TILE_PAD + (i % GRID_COLS) * (TILE_W + TILE_GAP),
                        top: TILE_PAD + (i / GRID_COLS) * (TILE_H + TILE_GAP),
                        width: TILE_W,
                        height: TILE_H
                    )
                }
            }
            Text (
                id: "offscreen_readout",
                "warming up...",
                width: Dimension::percent(100),
                max_width: 320,
                height: 24,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            ) [
                FpsReadout {
                    counter: 0,
                    accum_render_ns: 0,
                },
            ]
        }
    };
    //~focus-end
}
