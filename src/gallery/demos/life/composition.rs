use super::runtime::dims_from_px;
#[cfg(feature = "std")]
use super::runtime::install_runtime;
use super::state::seeded_board;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16) {
    let (cols, rows) = dims_from_px(view_w as i32, view_h as i32);
    let board = seeded_board(cols, rows);

    //~focus-start
    ui! {
        Column (
            id: "life_shell",
            bg_color: ColorToken::Surface,
            grow: 1.0,
            width: Dimension::percent(100),
            padding: Padding::all(14),
            row_gap: 10,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            Row (
                id: "life_header",
                width: Dimension::percent(100),
                max_width: 760,
                height: 44,
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Column (grow: 1.0, row_gap: 2) {
                    Text (
                        "CELLULAR FIELD",
                        width: Dimension::percent(100),
                        height: 24,
                        font_size: 20,
                        text_color: ColorToken::OnSurface
                    )
                    Text (
                        "toroidal life / seeded emitters",
                        width: Dimension::percent(100),
                        height: 16,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant
                    )
                }
                View (
                    width: 76,
                    height: 24,
                    padding: Padding {
                        top: Dimension::px(5),
                        right: Dimension::px(8),
                        bottom: Dimension::px(5),
                        left: Dimension::px(8),
                    },
                    direction: FlexDirection::Row,
                    align: AlignItems::Center,
                    column_gap: 5,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 12
                ) {
                    View (
                        width: 6,
                        height: 6,
                        bg_color: ColorToken::Success,
                        border_radius: 3
                    )
                    Text (
                        "LIVE",
                        grow: 1.0,
                        height: 14,
                        font_size: 9,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
            View (
                id: "life_stage",
                width: Dimension::percent(100),
                max_width: 760,
                min_height: 120,
                grow: 1.0,
                padding: Padding::all(8),
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 16
            ) {
                View (
                    id: "life_board",
                    width: Dimension::percent(100),
                    grow: 1.0,
                    bg_color: ColorToken::Surface,
                    border_radius: 10,
                    clip_children: true
                ) [
                    board,
                ]
            }
            Text (
                "GOSPER GUN  ·  ACORN  ·  GLIDER FEED",
                width: Dimension::percent(100),
                max_width: 760,
                height: 18,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let info = app.backend.display_info();
    install_runtime(app);
    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));
}
