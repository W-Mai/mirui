use super::state::{
    FpsReadout, GRID_COLS, GRID_ROWS, MODAL_H, MODAL_W, ModalAnim, TILE_GAP, TILE_H, TILE_PAD,
    TILE_W,
};
use super::style::tile_color;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            row_gap: 6,
            bg_color: ColorToken::Surface,
            clip_children: true
        ) {
            View (
                id: "offscreen_modal_panel",
                bg_color: ColorToken::SurfaceVariant,
                border_radius: Fixed::from_int(18),
                width: MODAL_W,
                height: MODAL_H
            ) [
                ModalAnim { t: Fixed::ZERO },
            ] {
                walk 0..(GRID_COLS * GRID_ROWS) with i {
                    View (
                        bg_color: tile_color(i),
                        border_radius: Fixed::from_int(11),
                        position: Position::Absolute,
                        left: TILE_PAD + (i % GRID_COLS) * (TILE_W + TILE_GAP),
                        top: TILE_PAD + (i / GRID_COLS) * (TILE_H + TILE_GAP),
                        width: TILE_W,
                        height: TILE_H
                    )
                }
            }
            Text (
                id: "offscreen_modal_readout",
                "warming up...",
                width: Dimension::percent(100),
                max_width: 320,
                height: 24,
                font_size: 13,
                text_color: ColorToken::OnSurfaceVariant,
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
