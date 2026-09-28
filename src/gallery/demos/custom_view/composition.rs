#![allow(clippy::needless_update)]

use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

use super::view::{Diamond, PALETTE};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 16,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "CUSTOM VECTOR VIEW",
                width: Dimension::percent(100),
                max_width: 480,
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "STATIC PATH · TAP ANY TILE TO RECOLOR",
                width: Dimension::percent(100),
                max_width: 480,
                height: 18,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "custom_view_tiles",
                justify: JustifyContent::SpaceEvenly,
                align: AlignItems::Center,
                width: Dimension::percent(100),
                max_width: 480,
                height: 120
            ) {
                Diamond (
                    color: PALETTE[0],
                    line_width: Fixed::from_int(2),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
                Diamond (
                    color: PALETTE[1],
                    line_width: Fixed::from_int(3),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
                Diamond (
                    color: PALETTE[2],
                    line_width: Fixed::from_int(4),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
            }
        }
    };
    //~focus-end
}
