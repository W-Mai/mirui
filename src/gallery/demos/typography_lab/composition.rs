use super::caret::CaretOverlay;
#[cfg(feature = "std")]
use super::caret::caret_overlay_view;
use super::contour::RasterContour;
#[cfg(feature = "std")]
use super::contour::raster_contour_view;
use super::runtime::{ARABIC, CJK, DEVANAGARI, FEATURES_OFF, LIVE_SAMPLE, THAI, UI, mixed_stack};
#[cfg(feature = "std")]
use super::runtime::{register_fonts, register_path};
use super::state::{TypographyModel, align_label, live_paragraph, overflow_label, wrap_label};
use super::style::{
    ACTIVE_RENDER_PATH, BACKGROUND, BLUE, BORDER, CYAN, GOLD, MUTED, PANEL, PANEL_ALT, TEXT, VIOLET,
};
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{
    Button, FontFeatures, LanguageTag, ParagraphStyle, ShapingPolicy, Slider, Text, TextDirection,
    TextOverflow, TextWrap, WidgetTransform3D,
};

fn paragraph(language: Option<&'static str>, direction: TextDirection) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        direction,
        language: language.map(|tag| LanguageTag::parse(tag).expect("language tag")),
        shaping: ShapingPolicy::Required,
        ..ParagraphStyle::default()
    }
}

fn plain_paragraph() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn bounded_paragraph(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: if lines == 1 {
            TextWrap::NoWrap
        } else {
            TextWrap::Word
        },
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        shaping: ShapingPolicy::Required,
        ..ParagraphStyle::default()
    }
}

fn features_off() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        max_lines: Some(1),
        features: FontFeatures::borrowed(&FEATURES_OFF).expect("bounded features"),
        ..ParagraphStyle::default()
    }
}

fn typography_grid_height(document_width: Fixed) -> Fixed {
    const PANEL_COUNT: i32 = 8;
    const PANEL_MIN_WIDTH: i32 = 220;
    const PANEL_HEIGHT: i32 = 186;
    const PATH_PANEL_EXTRA_HEIGHT: i32 = 26;
    const GAP: i32 = 12;
    const DOCUMENT_HORIZONTAL_PADDING: i32 = 36;

    let available = (document_width.to_int() - DOCUMENT_HORIZONTAL_PADDING).max(1);
    let columns = ((available + GAP) / (PANEL_MIN_WIDTH + GAP)).clamp(1, PANEL_COUNT);
    let rows = (PANEL_COUNT + columns - 1) / columns;
    Fixed::from_int(rows * PANEL_HEIGHT + PATH_PANEL_EXTRA_HEIGHT + (rows - 1) * GAP)
}

fn typography_controls_height(document_width: Fixed) -> Fixed {
    const DOCUMENT_HORIZONTAL_PADDING: i32 = 36;
    const CONTROL_HORIZONTAL_PADDING: i32 = 24;
    const COLUMN_MIN_WIDTH: i32 = 250;
    const COLUMN_GAP: i32 = 14;
    const LIVE_COLUMN_HEIGHT: i32 = 130;
    const SETTINGS_COLUMN_HEIGHT: i32 = 100;
    const ROW_GAP: i32 = 12;

    let inner_width =
        document_width.to_int() - DOCUMENT_HORIZONTAL_PADDING - CONTROL_HORIZONTAL_PADDING;
    let content_height = if inner_width >= COLUMN_MIN_WIDTH * 2 + COLUMN_GAP {
        LIVE_COLUMN_HEIGHT
    } else {
        LIVE_COLUMN_HEIGHT + ROW_GAP + SETTINGS_COLUMN_HEIGHT
    };
    Fixed::from_int(content_height + CONTROL_HORIZONTAL_PADDING)
}

#[compose(bind(model))]
fn build_widgets(model: TypographyModel, wave_path: PathId) {
    //~focus-start
    ui! {
        Scroll (
            id: "typography_lab_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "typography_lab_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(18),
                row_gap: 14
            ) {
                Row (
                    id: "typography_lab_header",
                    min_height: 70,
                    wrap: FlexWrap::Wrap,
                    align: AlignItems::Center,
                    row_gap: 4,
                    column_gap: 14
                ) {
                    View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
                    Column (grow: 1.0, min_width: 150, row_gap: 3) {
                        Text (
                            "TYPOGRAPHY LAB",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 24,
                            text_color: TEXT,
                            paragraph: bounded_paragraph(1)
                        )
                        Text (
                            "中文排版 · borrowed MIRX · bounded shaping",
                            width: Dimension::percent(100),
                            min_height: 18,
                            font_stack: mixed_stack(),
                            font_size: 13,
                            text_color: MUTED,
                            paragraph: bounded_paragraph(2)
                        )
                    }
                    Text (
                        id: "typography_panel_count",
                        "8 TEST PANELS",
                        width: Dimension::percent(32),
                        min_width: 96,
                        max_width: 158,
                        height: 30,
                        bg_color: PANEL_ALT,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 15,
                        font: UI,
                        font_size: 12,
                        text_color: CYAN,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Row (
                    id: "typography_lab_grid",
                    width: Dimension::percent(100),
                    height: @id(typography_lab_document).width {
                        typography_grid_height(typography_lab_document.width)
                    },
                    wrap: FlexWrap::Wrap,
                    align: AlignItems::FlexStart,
                    row_gap: 12,
                    column_gap: 12
                ) {
                    Column (
                        id: "typography_latin",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 8,
                        clip_children: true,
                        bg_color: PANEL,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("LATIN · GSUB / GPOS", font: UI, font_size: 12, text_color: BLUE)
                        Text (
                            id: "typography_latin_shaped",
                            "office ffi · AVATAR To",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 27,
                            text_color: TEXT,
                            paragraph: plain_paragraph()
                        )
                        Text (
                            id: "typography_latin_plain",
                            "office ffi · AVATAR To",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 17,
                            text_color: MUTED,
                            paragraph: features_off()
                        )
                        Text (
                            "top liga kern · bottom disabled",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 12,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_cjk",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 9,
                        clip_children: true,
                        bg_color: PANEL_ALT,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("CJK · GLYPH METRICS", font: UI, font_size: 12, text_color: GOLD)
                        Text (
                            id: "typography_cjk_sample",
                            "中文字体排版",
                            width: Dimension::percent(100),
                            font: CJK,
                            font_size: 30,
                            text_color: TEXT,
                            paragraph: plain_paragraph()
                        )
                        Text (
                            "真实 bearing · advance · atlas bounds",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 13,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_arabic",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 9,
                        clip_children: true,
                        bg_color: PANEL,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text (
                            "ARABIC · RTL JOINING",
                            font: UI,
                            font_size: 12,
                            text_color: VIOLET
                        )
                        Text (
                            id: "typography_arabic_sample",
                            "مَرْحَبًا بِالْعَالَمِ",
                            width: Dimension::percent(100),
                            font: ARABIC,
                            font_size: 30,
                            text_color: TEXT,
                            paragraph: paragraph(Some("ar"), TextDirection::RightToLeft)
                        )
                        Text (
                            "joining · cursive · mark anchors",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 13,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_thai",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 9,
                        clip_children: true,
                        bg_color: PANEL_ALT,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("THAI · MARK PLACEMENT", font: UI, font_size: 12, text_color: CYAN)
                        Text (
                            id: "typography_thai_sample",
                            "สวัสดีครับ · ตั้ง",
                            width: Dimension::percent(100),
                            font: THAI,
                            font_size: 27,
                            text_color: TEXT,
                            paragraph: paragraph(Some("th"), TextDirection::LeftToRight)
                        )
                        Text (
                            "decomposition · GDEF mark filtering",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 13,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_devanagari",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 9,
                        clip_children: true,
                        bg_color: PANEL,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text (
                            "DEVANAGARI · CONJUNCTS",
                            font: UI,
                            font_size: 12,
                            text_color: GOLD
                        )
                        Text (
                            id: "typography_devanagari_sample",
                            "किरण · क्षत्रिय",
                            width: Dimension::percent(100),
                            font: DEVANAGARI,
                            font_size: 27,
                            text_color: TEXT,
                            paragraph: paragraph(Some("hi"), TextDirection::LeftToRight)
                        )
                        Text (
                            "pre-base matra · conjunct forms",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 13,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_bidi",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 9,
                        clip_children: true,
                        bg_color: PANEL,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("MIXED BIDI · FALLBACK", font: UI, font_size: 12, text_color: BLUE)
                        Text (
                            id: "typography_bidi_sample",
                            "mirui 42 · 中文字体排版 · مرحبا",
                            width: Dimension::percent(100),
                            font_stack: mixed_stack(),
                            font_size: 21,
                            text_color: TEXT,
                            paragraph: ParagraphStyle {
                                max_lines: Some(1),
                                overflow: TextOverflow::Ellipsis,
                                ..paragraph(None, TextDirection::Auto)
                            }
                        )
                        Text (
                            "grapheme-safe face selection",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 13,
                            text_color: MUTED
                        )
                    }
                    Column (
                        id: "typography_rasters",
                        grow: 1.0,
                        min_width: 220,
                        height: 186,
                        padding: Padding::all(14),
                        row_gap: 7,
                        clip_children: true,
                        bg_color: PANEL_ALT,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("COVERAGE / SDF", font: UI, font_size: 12, text_color: GOLD)
                        RasterContour (
                            id: "typography_contour",
                            font: UI,
                            character: 'S',
                            ppem: 56u16,
                            height: 88
                        )
                        Text (
                            "midpoint contour · actual packed A8 samples",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 11,
                            text_color: MUTED
                        )
                        Text (
                            ACTIVE_RENDER_PATH,
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 10,
                            text_color: VIOLET,
                            paragraph: plain_paragraph()
                        )
                    }
                    Column (
                        id: "typography_path",
                        grow: 1.0,
                        min_width: 220,
                        height: 212,
                        padding: Padding::all(14),
                        row_gap: 6,
                        clip_children: true,
                        bg_color: PANEL,
                        border_color: BORDER,
                        border_width: 1,
                        border_radius: 14
                    ) {
                        Text ("PATH + PROJECTIVE", font: UI, font_size: 12, text_color: CYAN)
                        View (height: 78) {
                            Text (
                                id: "typography_path_sample",
                                "mirui 42 · مرحبا",
                                path: wave_path,
                                position: Position::Absolute,
                                left: 0,
                                top: 0,
                                width: 190,
                                height: 78,
                                font_stack: mixed_stack(),
                                font_size: 17,
                                text_color: TEXT,
                                paragraph: paragraph(None, TextDirection::Auto)
                            ) [
                                CaretOverlay {
                                    model: model.clone(),
                                    follows_probe: true,
                                },
                            ] on Tap { model.set_path_probe(Point { x: *x, y: *y }); } on DragMove { model.set_path_probe(Point { x: *x, y: *y }); }
                        }
                        View (id: "typography_projective_frame", height: 48, clip_children: true) [
                            WidgetTransform3D(
                                Transform3D::rotate_y_perspective(Fixed::from_int(-12), Fixed::from_int(500)),
                            ),
                        ] {
                            Text (
                                id: "typography_projective_sample",
                                "2.5D",
                                width: 190,
                                height: 48,
                                font: UI,
                                font_size: 20,
                                text_color: GOLD,
                                paragraph: ParagraphStyle::label()
                            )
                        }
                        Text (
                            text: format_args!(
                                "POSE {} B RAM / {} B WIRE · MATRIX {} B\nNATIVE OR CALLER-BUDGETED FALLBACK",
                                core::mem::size_of::<textflow::placement::GlyphFrame>(),
                                mirx::scene::GlyphPose::WIRE_SIZE,
                                mirx::types::Transform3D::WIRE_SIZE,
                            ),
                            text_capacity: 96,
                            width: Dimension::percent(100),
                            height: 24,
                            font: UI,
                            font_size: 9,
                            text_color: MUTED
                        )
                    }
                }
                Row (
                    id: "typography_controls",
                    width: Dimension::percent(100),
                    height: @id(typography_lab_document).width {
                        typography_controls_height(typography_lab_document.width)
                    },
                    min_height: 154,
                    padding: Padding::all(12),
                    wrap: FlexWrap::Wrap,
                    row_gap: 12,
                    column_gap: 14,
                    clip_children: true,
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Column (
                        grow: 1.0,
                        width: Dimension::percent(56),
                        min_width: 250,
                        height: 130,
                        row_gap: 6
                    ) {
                        Text ("LIVE PARAGRAPH", font: UI, font_size: 12, text_color: CYAN)
                        View (grow: 1.0, height: 78) {
                            Text (
                                id: "typography_live_sample",
                                LIVE_SAMPLE,
                                position: Position::Absolute,
                                left: 0,
                                top: 0,
                                width: ${ model.width() },
                                height: 78,
                                font_stack: mixed_stack(),
                                font_size: ${ model.ppem() },
                                text_color: TEXT,
                                paragraph: ${ live_paragraph(model.wrap(), model.align(), model.overflow()) }
                            ) [
                                CaretOverlay {
                                    model: model.clone(),
                                    follows_probe: false,
                                },
                            ]
                        }
                        Text (
                            "cyan LTR · violet RTL · lines are authoritative caret stops",
                            width: Dimension::percent(100),
                            font: UI,
                            font_size: 11,
                            text_color: MUTED,
                            paragraph: bounded_paragraph(1)
                        )
                    }
                    Column (
                        grow: 1.0,
                        width: Dimension::percent(40),
                        min_width: 250,
                        height: 100,
                        row_gap: 7
                    ) {
                        Row (height: 22, align: AlignItems::Center, column_gap: 9) {
                            Text ("PPEM", width: 54, font: UI, font_size: 11, text_color: MUTED)
                            Slider (
                                id: "typography_ppem",
                                grow: 1.0,
                                height: 12,
                                min: Fixed::from_int(10),
                                max: Fixed::from_int(64),
                                value: ${ Fixed::from_int(i32::from(model.ppem())) },
                                track_color: BORDER,
                                fill_color: CYAN,
                                thumb_color: TEXT
                            ) on ValueChanged {
                                let _ = old;
                                model.set_ppem(*new);
                            }
                            Text (
                                id: "typography_ppem_value",
                                text: ${ format_args!("{}", model.ppem()) },
                                text_capacity: 2,
                                width: 34,
                                font: UI,
                                font_size: 11,
                                text_color: TEXT
                            )
                        }
                        Row (height: 22, align: AlignItems::Center, column_gap: 9) {
                            Text ("WIDTH", width: 54, font: UI, font_size: 11, text_color: MUTED)
                            Slider (
                                id: "typography_width",
                                grow: 1.0,
                                height: 12,
                                min: Fixed::from_int(220),
                                max: Fixed::from_int(560),
                                value: ${ Fixed::from_int(i32::from(model.width())) },
                                track_color: BORDER,
                                fill_color: BLUE,
                                thumb_color: TEXT
                            ) on ValueChanged {
                                let _ = old;
                                model.set_width(*new);
                            }
                            Text (
                                id: "typography_width_value",
                                text: ${ format_args!("{}", model.width()) },
                                text_capacity: 3,
                                width: 34,
                                font: UI,
                                font_size: 11,
                                text_color: TEXT
                            )
                        }
                        Row (height: 28, column_gap: 7) {
                            Button (
                                id: "typography_wrap",
                                text: ${ wrap_label(model.wrap()) },
                                text_capacity: 8,
                                grow: 1.0,
                                height: 28,
                                normal_color: PANEL_ALT,
                                pressed_color: CYAN,
                                border_color: BORDER,
                                border_width: 1,
                                border_radius: 8,
                                font: UI,
                                font_size: 10,
                                text_color: CYAN
                            ) on Tap { model.cycle_wrap(); }
                            Button (
                                id: "typography_align",
                                text: ${ align_label(model.align()) },
                                text_capacity: 7,
                                grow: 1.0,
                                height: 28,
                                normal_color: PANEL_ALT,
                                pressed_color: BLUE,
                                border_color: BORDER,
                                border_width: 1,
                                border_radius: 8,
                                font: UI,
                                font_size: 10,
                                text_color: BLUE
                            ) on Tap { model.cycle_align(); }
                            Button (
                                id: "typography_overflow",
                                text: ${ overflow_label(model.overflow()) },
                                text_capacity: 8,
                                grow: 1.0,
                                height: 28,
                                normal_color: PANEL_ALT,
                                pressed_color: GOLD,
                                border_color: BORDER,
                                border_width: 1,
                                border_radius: 8,
                                font: UI,
                                font_size: 10,
                                text_color: GOLD
                            ) on Tap { model.toggle_overflow(); }
                        }
                    }
                }
            }
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
    crate::gallery::showcase_theme::install(&mut app.world);
    app.with_widget(caret_overlay_view());
    app.with_widget(raster_contour_view());
    register_fonts(&mut app.world);
    let wave_path = register_path(&mut app.world);
    let model = app.add_model(TypographyModel::default());
    app.compose(parent, |cx| build_widgets(cx, model, wave_path));
}
