use super::chips::CHIPS;
use super::style::{
    BACKGROUND, BLUE, BORDER, CYAN, GOLD, MUTED, PANEL, PANEL_ALT, TEXT, VIOLET, bounded_body,
};
use crate::prelude::*;
use crate::ui::widgets::{Image, ParagraphStyle, Text};

#[compose]
pub(super) fn compose_header() -> Entity {
    ui! {
        Row (
            id: "layout_lab_header",
            min_height: @id(layout_lab_document).width {
                if layout_lab_document.width < Fixed::from_int(640) { 108 } else { 70 }
            },
            wrap: FlexWrap::Wrap,
            align: AlignItems::Center,
            row_gap: 4,
            column_gap: 14
        ) {
            View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
            Column (grow: 1.0, min_width: 150, row_gap: 3) {
                Text (
                    "LAYOUT LAB",
                    width: Dimension::percent(100),
                    font_size: 24,
                    text_color: TEXT,
                    paragraph: bounded_body(1)
                )
                Text (
                    "responsive geometry / one widget tree",
                    width: Dimension::percent(100),
                    min_height: 18,
                    font_size: 13,
                    text_color: MUTED,
                    paragraph: bounded_body(2)
                )
            }
            Text (
                "8 CAPABILITIES",
                width: @id(layout_lab_document).width {
                    if layout_lab_document.width < Fixed::from_int(640) {
                        Dimension::percent(100)
                    } else {
                        Dimension::percent(32)
                    }
                },
                min_width: 96,
                max_width: @id(layout_lab_document).width {
                    if layout_lab_document.width < Fixed::from_int(640) {
                        Dimension::percent(100)
                    } else {
                        Dimension::px(154)
                    }
                },
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font_size: 12,
                text_color: CYAN,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
pub(super) fn compose_flex_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_flex",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 300,
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "FLEX / CONTENT > GROW > LIMITS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: BLUE,
                paragraph: bounded_body(2)
            )
            Row (
                min_height: 70,
                wrap: FlexWrap::Wrap,
                align: AlignItems::Center,
                row_gap: 8,
                column_gap: 8
            ) {
                Text (
                    id: "layout_lab_content",
                    "CONTENT",
                    width: Dimension::Content,
                    height: 38,
                    padding: Padding::all(8),
                    bg_color: CYAN,
                    border_radius: 9,
                    font_size: 11,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_grow",
                    "GROW 1",
                    grow: 1.0,
                    min_width: 88,
                    max_width: 220,
                    height: 48,
                    bg_color: BLUE,
                    border_radius: 11,
                    font_size: 12,
                    text_color: ColorToken::OnSecondary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_fixed",
                    "96 PX",
                    width: 96,
                    height: 32,
                    bg_color: VIOLET,
                    border_radius: 8,
                    font_size: 11,
                    text_color: ColorToken::OnTertiary,
                    paragraph: ParagraphStyle::label()
                )
            }
            Row (
                id: "layout_lab_wrap",
                wrap: FlexWrap::Wrap,
                height: 100,
                align: AlignItems::Center,
                row_gap: 7,
                column_gap: 7
            ) {
                walk CHIPS.iter() with chip {
                    Text (
                        chip.label,
                        width: 76,
                        height: 28,
                        bg_color: PANEL_ALT,
                        border_color: chip.color,
                        border_width: 1,
                        border_radius: 14,
                        font_size: 9,
                        text_color: chip.color,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
            Text (
                "The same row wraps when its minimum widths no longer fit.",
                width: Dimension::percent(100),
                min_height: 32,
                font_size: 11,
                text_color: MUTED,
                paragraph: bounded_body(2)
            )
        }
    }
}

#[compose]
pub(super) fn compose_surface_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_surfaces",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 12,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "SURFACES / BORDER / RADIUS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: GOLD,
                paragraph: bounded_body(2)
            )
            Row (grow: 1.0, align: AlignItems::Center, justify: JustifyContent::SpaceEvenly) {
                View (
                    id: "layout_lab_surface_round",
                    width: 72,
                    height: 72,
                    bg_color: BLUE,
                    border_color: ColorToken::OnSecondary,
                    border_width: 2,
                    border_radius: 18
                )
                View (
                    width: 72,
                    height: 72,
                    bg_color: CYAN,
                    border_color: ColorToken::OnPrimary,
                    border_width: 3,
                    border_radius: 36
                )
                View (
                    width: 72,
                    height: 72,
                    border_color: VIOLET,
                    border_width: 3,
                    border_radius: 9
                )
            }
            Text (
                id: "layout_lab_surface_note",
                "Fill, stroke and radius stay independent.",
                width: Dimension::percent(100),
                height: 28,
                font_size: 11,
                text_color: MUTED,
                paragraph: bounded_body(2)
            )
        }
    }
}

#[compose]
pub(super) fn compose_overlay_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_overlay_card",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 8,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "OVERLAY / ABSOLUTE IN A FLEX CARD",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: VIOLET,
                paragraph: bounded_body(2)
            )
            View (
                id: "layout_lab_overlay_stage",
                grow: 1.0,
                min_height: 162,
                bg_color: BACKGROUND,
                border_radius: 11,
                clip_children: true
            ) {
                View (
                    width: Dimension::percent(72),
                    height: Dimension::percent(62),
                    bg_color: ColorToken::Primary,
                    border_radius: 12
                )
                View (
                    id: "layout_lab_absolute",
                    position: Position::Absolute,
                    left: 24,
                    top: 46,
                    width: 126,
                    height: 58,
                    bg_color: BLUE,
                    border_radius: 13
                )
                Image (
                    id: "layout_lab_image_overlay",
                    position: Position::Absolute,
                    left: 166,
                    top: 28,
                    width: 56,
                    height: 56,
                    src: "thumbs_up"
                )
                Text (
                    "PINNED",
                    position: Position::Absolute,
                    right: 10,
                    top: 104,
                    width: 88,
                    height: 30,
                    bg_color: VIOLET,
                    border_radius: 15,
                    font_size: 10,
                    text_color: ColorToken::OnTertiary,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
pub(super) fn compose_collection_card() -> Entity {
    let show_note = true;

    ui! {
        Column (
            id: "layout_lab_collection",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "COMPOSITION / WALK / IF / IMAGE",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: CYAN,
                paragraph: bounded_body(2)
            )
            Row (height: 82, align: AlignItems::Center, column_gap: 10) {
                Image (
                    id: "layout_lab_image_flow",
                    width: 58,
                    height: 58,
                    src: "thumbs_up"
                )
                Column (grow: 1.0, row_gap: 6) {
                    Text (
                        "Typed image stays in flex flow.",
                        width: Dimension::percent(100),
                        font_size: 12,
                        text_color: TEXT,
                        paragraph: bounded_body(2)
                    )
                    Text (
                        "Overlay reuses its resource.",
                        width: Dimension::percent(100),
                        font_size: 11,
                        text_color: MUTED,
                        paragraph: bounded_body(2)
                    )
                }
            }
            Row (id: "layout_lab_walk", height: 42, column_gap: 7) {
                walk CHIPS.iter() with chip {
                    View (grow: 1.0, height: 20, bg_color: chip.color, border_radius: 6)
                }
            }
            if show_note {
                Text (
                    id: "layout_lab_conditional",
                    "Conditional branch retained",
                    height: 30,
                    bg_color: ColorToken::Primary,
                    border_radius: 8,
                    font_size: 11,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}
