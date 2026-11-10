use super::chips::CHIPS;
use super::style::{
    ACCENT, BACKGROUND, BORDER, MUTED, PANEL, PANEL_ALT, RAISED, SLATE, TEXT, bounded_body,
};
use crate::gallery::brand::MiruiMark;
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
            View (width: 32, height: 32) [
                MiruiMark,
            ]
            Column (grow: 1.0, height: 54, min_width: 150, row_gap: 3) {
                Text (
                    "Layout Lab",
                    width: Dimension::percent(100),
                    height: 31,
                    font_size: 22,
                    text_color: TEXT,
                    paragraph: bounded_body(1)
                )
                Text (
                    "Responsive widget tree",
                    width: Dimension::percent(100),
                    height: 20,
                    font_size: 13,
                    text_color: MUTED,
                    paragraph: bounded_body(1)
                )
            }
            Text (
                "8 capabilities",
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
                        Dimension::px(140)
                    }
                },
                height: 28,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 14,
                font_size: 12,
                text_color: MUTED,
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
            border_radius: 10
        ) {
            Text (
                "Flex: content, grow and limits",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 14,
                text_color: TEXT,
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
                    "Content",
                    width: Dimension::Content,
                    height: 36,
                    padding: Padding::all(10),
                    bg_color: ACCENT,
                    border_radius: 8,
                    font_size: 12,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_grow",
                    "Grow 1",
                    grow: 1.0,
                    min_width: 88,
                    max_width: 220,
                    height: 44,
                    bg_color: RAISED,
                    border_radius: 8,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_fixed",
                    "96 px",
                    width: 96,
                    height: 32,
                    bg_color: PANEL_ALT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 8,
                    font_size: 12,
                    text_color: TEXT,
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
                        height: 26,
                        bg_color: PANEL_ALT,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 13,
                        font_size: 11,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
            Text (
                "The same row wraps when its minimum widths no longer fit.",
                width: Dimension::percent(100),
                min_height: 32,
                font_size: 12,
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
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) {
            Text (
                "Fill, stroke and radius",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 14,
                text_color: TEXT,
                paragraph: bounded_body(2)
            )
            Row (grow: 1.0, align: AlignItems::Center, justify: JustifyContent::SpaceEvenly) {
                View (
                    id: "layout_lab_surface_round",
                    width: 72,
                    height: 72,
                    bg_color: ACCENT,
                    border_color: PANEL,
                    border_width: 2,
                    border_radius: 18
                )
                View (
                    width: 72,
                    height: 72,
                    bg_color: RAISED,
                    border_color: SLATE,
                    border_width: 3,
                    border_radius: 36
                )
                View (
                    width: 72,
                    height: 72,
                    border_color: MUTED,
                    border_width: 2,
                    border_radius: 9
                )
            }
            Text (
                id: "layout_lab_surface_note",
                "Fill, stroke and radius stay independent.",
                width: Dimension::percent(100),
                height: 28,
                font_size: 12,
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
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) {
            Text (
                "Absolute overlay in a flex card",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 14,
                text_color: TEXT,
                paragraph: bounded_body(2)
            )
            View (
                id: "layout_lab_overlay_stage",
                grow: 1.0,
                min_height: 162,
                bg_color: BACKGROUND,
                border_radius: 8,
                clip_children: true
            ) {
                View (
                    width: Dimension::percent(72),
                    height: Dimension::percent(62),
                    bg_color: RAISED,
                    border_radius: 8
                )
                View (
                    id: "layout_lab_absolute",
                    position: Position::Absolute,
                    left: 24,
                    top: 46,
                    width: 126,
                    height: 58,
                    bg_color: SLATE,
                    border_radius: 8
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
                    "Pinned",
                    position: Position::Absolute,
                    right: 10,
                    top: 104,
                    width: 80,
                    height: 28,
                    bg_color: ACCENT,
                    border_radius: 14,
                    font_size: 12,
                    text_color: ColorToken::OnPrimary,
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
            border_radius: 10
        ) {
            Text (
                "Composition: walk, if and image",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 14,
                text_color: TEXT,
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
                        font_size: 13,
                        text_color: TEXT,
                        paragraph: bounded_body(2)
                    )
                    Text (
                        "Overlay reuses its resource.",
                        width: Dimension::percent(100),
                        font_size: 12,
                        text_color: MUTED,
                        paragraph: bounded_body(2)
                    )
                }
            }
            Row (id: "layout_lab_walk", height: 42, align: AlignItems::Center, column_gap: 6) {
                walk CHIPS.iter() with chip {
                    View (grow: 1.0, height: 6, bg_color: chip.color, border_radius: 3)
                }
            }
            if show_note {
                Text (
                    id: "layout_lab_conditional",
                    "Conditional branch retained",
                    height: 28,
                    padding: Padding::all(10),
                    bg_color: RAISED,
                    border_radius: 6,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}
