extern crate alloc;

use alloc::format;

use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::font::scalar::ScalarField;
use crate::render::font::{Font, FontManager, FontStack, ResolvedFontStack};
use crate::render::renderer::Renderer;
use crate::types::{Transform, Transform3D};
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::text::FontFeature;
use crate::ui::widgets::{
    FontFeatures, LanguageTag, ParagraphStyle, ShapingPolicy, Slider, Text, TextAlign,
    TextDirection, TextOverflow, TextWrap, WidgetTransform3D,
};

pub const VIEWPORT: (u16, u16) = (1024, 720);

const UI_FONT: &[u8] = include_bytes!("assets/misans_ui.mirx");
const CJK_FONT: &[u8] = include_bytes!("assets/typography_cjk.mirx");
const ARABIC_FONT: &[u8] = include_bytes!("assets/typography_arabic.mirx");
const DEVANAGARI_FONT: &[u8] = include_bytes!("assets/typography_devanagari.mirx");
const THAI_FONT: &[u8] = include_bytes!("assets/typography_thai.mirx");
const ELLIPSIS_FONT: &[u8] = include_bytes!("assets/typography_ellipsis.mirx");

const UI: FontToken = FontToken::Custom("typography_ui");
const CJK: FontToken = FontToken::Custom("typography_cjk");
const ARABIC: FontToken = FontToken::Custom("typography_arabic");
const DEVANAGARI: FontToken = FontToken::Custom("typography_devanagari");
const THAI: FontToken = FontToken::Custom("typography_thai");
const ELLIPSIS: FontToken = FontToken::Custom("typography_ellipsis");
const FALLBACKS: [FontToken; 5] = [CJK, ARABIC, DEVANAGARI, THAI, ELLIPSIS];
const FEATURES_OFF: [FontFeature; 2] =
    [FontFeature::new(*b"liga", 0), FontFeature::new(*b"kern", 0)];
const LIVE_SAMPLE: &str = "office AVATAR · 中文字体排版 · مرحبا · किरण · ภาษาไทย";

static WAVE_BASELINE: Path = path!(M 4 68 C 38 16 92 14 126 50 C 148 74 170 68 188 34);

#[derive(Default, crate::Component)]
struct CaretOverlay {
    target: &'static str,
    probe: Option<Point>,
}

#[derive(crate::Component)]
struct RasterContour {
    font: FontToken,
    character: char,
    ppem: u16,
}

impl Default for RasterContour {
    fn default() -> Self {
        Self {
            font: UI,
            character: 'S',
            ppem: 56,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TypographyState {
    ppem: u16,
    width: u16,
    wrap: TextWrap,
    align: TextAlign,
    overflow: TextOverflow,
}

impl Default for TypographyState {
    fn default() -> Self {
        Self {
            ppem: 24,
            width: 480,
            wrap: TextWrap::Word,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
        }
    }
}

impl TypographyState {
    fn paragraph(self) -> ParagraphStyle {
        ParagraphStyle {
            wrap: self.wrap,
            align: self.align,
            overflow: self.overflow,
            max_lines: Some(2),
            shaping: ShapingPolicy::Required,
            ..ParagraphStyle::default()
        }
    }

    fn wrap_label(self) -> &'static str {
        match self.wrap {
            TextWrap::NoWrap => "NO WRAP",
            TextWrap::Word => "WORD",
            TextWrap::Grapheme => "GRAPHEME",
        }
    }

    fn align_label(self) -> &'static str {
        match self.align {
            TextAlign::Start => "START",
            TextAlign::Center => "CENTER",
            TextAlign::End => "END",
            TextAlign::Justify => "JUSTIFY",
        }
    }

    fn overflow_label(self) -> &'static str {
        match self.overflow {
            TextOverflow::Clip => "CLIP",
            TextOverflow::Ellipsis => "ELLIPSIS",
        }
    }
}

enum TypographyAction {
    SetPpem(Fixed),
    SetWidth(Fixed),
    CycleWrap,
    CycleAlign,
    ToggleOverflow,
}

impl TypographyAction {
    fn publish(self, state: &Signal<TypographyState>) {
        let mut next = state.get_untracked();
        match self {
            Self::SetPpem(value) => {
                next.ppem = value.round().to_int().clamp(10, 64) as u16;
            }
            Self::SetWidth(value) => {
                next.width = value.round().to_int().clamp(220, 560) as u16;
            }
            Self::CycleWrap => {
                next.wrap = match next.wrap {
                    TextWrap::NoWrap => TextWrap::Word,
                    TextWrap::Word => TextWrap::Grapheme,
                    TextWrap::Grapheme => TextWrap::NoWrap,
                };
            }
            Self::CycleAlign => {
                next.align = match next.align {
                    TextAlign::Start => TextAlign::Center,
                    TextAlign::Center => TextAlign::End,
                    TextAlign::End => TextAlign::Justify,
                    TextAlign::Justify => TextAlign::Start,
                };
            }
            Self::ToggleOverflow => {
                next.overflow = match next.overflow {
                    TextOverflow::Clip => TextOverflow::Ellipsis,
                    TextOverflow::Ellipsis => TextOverflow::Clip,
                };
            }
        }
        if next != state.get_untracked() {
            state.set(next);
        }
    }
}

const BACKGROUND: ColorToken = ColorToken::Surface;
const PANEL: ColorToken = ColorToken::SurfaceVariant;
const PANEL_ALT: ColorToken = ColorToken::Surface;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const CYAN: ColorToken = ColorToken::Primary;
const BLUE: ColorToken = ColorToken::Secondary;
const GOLD: ColorToken = ColorToken::Success;
const VIOLET: ColorToken = ColorToken::Tertiary;

const ACTIVE_RENDER_PATH: &str = "GLYPH RUN · A8 / SDF REPRESENTATIONS";

fn draw_line(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx<'_>,
    p1: Point,
    p2: Point,
    color: Color,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1,
            p2,
            transform: ctx.transform,
            color,
            width: Fixed::ONE,
            opa: 255,
        },
        ctx.clip,
    );
}

fn caret_overlay_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(overlay) = world.get::<CaretOverlay>(entity) else {
        return;
    };
    let Some(target) = world.find_by_id(overlay.target) else {
        return;
    };
    let (Some(style), Some(handle), Some(resource)) = (
        world.get::<crate::ui::Style>(target),
        world.get::<crate::text::TextLayoutHandle>(target).copied(),
        world.resource::<crate::text::layout::TextLayoutResource>(),
    ) else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let Ok(Some(fonts)) =
        ResolvedFontStack::resolve(world, &style.font_stack, style.font_size, face_limit)
    else {
        return;
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let cache = resource.borrow();
    let Some(layout) = cache.get(handle) else {
        return;
    };
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let border = theme.resolve(BORDER);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let tertiary = theme.resolve(VIOLET);
    let success = theme.resolve(GOLD);
    if let Some(text_path) = world.get::<TextPath>(target).copied() {
        let Some(geometry) = crate::text::PathTextGeometry::for_widget(world, target) else {
            return;
        };
        let probe_hit = overlay
            .probe
            .and_then(|probe| geometry.hit_test(probe, Fixed::from_int(16)).ok().flatten());
        let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
            return;
        };
        if let Ok(path) = paths.get(text_path.path()) {
            let paint = Paint::Color(secondary.into());
            ctx.draw(
                renderer,
                &DrawCommand::StrokePath {
                    path,
                    transform: ctx.transform.compose(&Transform::translate(rect.x, rect.y)),
                    paint: &paint,
                    width: Fixed::ONE,
                    opa: 120,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    miter_limit: Fixed::from_int(4),
                    dash: &[],
                },
                ctx.clip,
            );
        }
        let mut caret_storage = [crate::text::PathCaretGeometry::default(); 128];
        if let Ok(carets) = geometry.carets_into(&mut caret_storage) {
            for caret in carets {
                let [start, end] = caret.segment();
                draw_line(
                    renderer,
                    ctx,
                    start,
                    end,
                    if probe_hit.is_some_and(|hit| hit.index() == caret.index()) {
                        success
                    } else if caret.bidi_level() & 1 == 0 {
                        primary
                    } else {
                        tertiary
                    },
                );
            }
        }
        return;
    }
    for line in layout.lines() {
        let baseline = rect.y + crate::types::fixed::from_textflow(line.origin().y);
        draw_line(
            renderer,
            ctx,
            Point {
                x: rect.x,
                y: baseline,
            },
            Point {
                x: rect.x + rect.w,
                y: baseline,
            },
            border,
        );
    }
    for caret in layout.carets() {
        let x = rect.x + crate::types::fixed::from_textflow(caret.position.x);
        if x < rect.x || x > rect.x + rect.w {
            continue;
        }
        let baseline = rect.y + crate::types::fixed::from_textflow(caret.position.y);
        draw_line(
            renderer,
            ctx,
            Point {
                x,
                y: baseline - metrics.ascender,
            },
            Point {
                x,
                y: baseline - metrics.ascender + metrics.line_height,
            },
            if caret.bidi_level & 1 == 0 {
                primary
            } else {
                tertiary
            },
        );
    }
}

fn contour_color(theme: &Theme, value: Fixed) -> Color {
    let surface = theme.resolve(PANEL_ALT);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let success = theme.resolve(GOLD);
    match crate::types::fixed::to_textflow(value).clamp(0, 256) {
        0..=63 => surface,
        64..=111 => surface.blend_with(secondary, Fixed::from_ratio(1, 2)),
        112..=144 => success,
        145..=207 => secondary,
        _ => primary,
    }
}

fn raster_contour_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(spec), Some(manager)) = (
        world.get::<RasterContour>(entity),
        world.resource::<FontManager>(),
    ) else {
        return;
    };
    let font = manager.resolve(spec.font.cache_key());
    let Some(raster) = font
        .map_char(spec.character)
        .and_then(|glyph| font.raster(glyph, spec.ppem))
    else {
        return;
    };
    let Some(region) = raster.region else {
        return;
    };
    let bits = match raster.representation.kind() {
        mirx::font::FontRepresentationKind::Coverage { bits }
        | mirx::font::FontRepresentationKind::SignedDistance { bits, .. } => bits,
        _ => return,
    };
    let Some(field) = ScalarField::new(
        raster.surface.samples(),
        raster.surface.stride(),
        region,
        bits,
    ) else {
        return;
    };
    let width = i32::try_from(field.width()).unwrap_or(i32::MAX).max(1);
    let height = i32::try_from(field.height()).unwrap_or(i32::MAX).max(1);
    let cell = (rect.w.to_int() / width)
        .min(rect.h.to_int() / height)
        .max(1);
    let grid_width = Fixed::from_int(width * cell);
    let grid_height = Fixed::from_int(height * cell);
    let origin = Point {
        x: rect.x + (rect.w - grid_width) / Fixed::from_int(2),
        y: rect.y + (rect.h - grid_height) / Fixed::from_int(2),
    };
    let extent = Fixed::from_int((cell - 1).max(1));
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    for y in 0..height {
        for x in 0..width {
            ctx.draw(
                renderer,
                &DrawCommand::Fill {
                    area: Rect {
                        x: origin.x + Fixed::from_int(x * cell),
                        y: origin.y + Fixed::from_int(y * cell),
                        w: extent,
                        h: extent,
                    },
                    transform: ctx.transform,
                    quad: None,
                    color: contour_color(theme, field.sample(x, y)),
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                ctx.clip,
            );
        }
    }
}

pub fn caret_overlay_view() -> View {
    View::new("CaretOverlay", 61, caret_overlay_render)
}

pub fn raster_contour_view() -> View {
    View::new("RasterContour", 61, raster_contour_render)
}

fn font(bytes: &'static [u8], family: &'static str) -> Font {
    Font::from_mirx(family, 24, bytes, &mirx::reader::PayloadLimits::HOST)
        .expect("Typography Lab font")
}

pub fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    manager.add_static(UI.cache_key(), font(UI_FONT, "MiSans UI"));
    manager.add_static(CJK.cache_key(), font(CJK_FONT, "MiSans CJK"));
    manager.add_static(ARABIC.cache_key(), font(ARABIC_FONT, "MiSans Arabic"));
    manager.add_static(
        DEVANAGARI.cache_key(),
        font(DEVANAGARI_FONT, "Noto Sans Devanagari"),
    );
    manager.add_static(THAI.cache_key(), font(THAI_FONT, "Noto Sans Thai"));
    manager.add_static(ELLIPSIS.cache_key(), font(ELLIPSIS_FONT, "Noto Sans"));
}

pub fn register_path(world: &mut World) -> PathId {
    world
        .paths()
        .insert_static(WAVE_BASELINE.commands())
        .expect("static typography path")
}

fn set_path_probe(world: &mut World, point: Point) {
    let Some(entity) = world.find_by_id("typography_path_carets") else {
        return;
    };
    let Some(overlay) = world.get_mut::<CaretOverlay>(entity) else {
        return;
    };
    if overlay.probe != Some(point) {
        overlay.probe = Some(point);
        world.invalidate(entity);
    }
}

fn mixed_stack() -> FontStack {
    FontStack::new(UI).with_fallbacks(&FALLBACKS[..])
}

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

fn geometry_cost_label() -> alloc::string::String {
    format!(
        "POSE {} B RAM / 18 B WIRE · MATRIX 72 B\nSW/WGPU NATIVE · SDL/WEB 320 KiB MAX",
        core::mem::size_of::<textflow::placement::GlyphFrame>()
    )
}

fn features_off() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        max_lines: Some(1),
        features: FontFeatures::borrowed(&FEATURES_OFF).expect("bounded features"),
        ..ParagraphStyle::default()
    }
}

#[compose]
pub fn build_widgets(wave_path: PathId) {
    let state = Signal::new(TypographyState::default());
    let sample_width = state.clone();
    let caret_width = state.clone();
    let sample_ppem = state.clone();
    let sample_paragraph = state.clone();
    let ppem_value = state.clone();
    let width_value = state.clone();
    let wrap_value = state.clone();
    let align_value = state.clone();
    let overflow_value = state.clone();
    let ppem_action = state.clone();
    let width_action = state.clone();
    let wrap_action = state.clone();
    let align_action = state.clone();
    let overflow_action = state;

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 14,
            bg_color: BACKGROUND
        ) {
            Row (height: 62, align: AlignItems::Center, column_gap: 14) {
                View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
                Column (grow: 1.0, row_gap: 3) {
                    Text (
                        "TYPOGRAPHY LAB",
                        font: UI,
                        font_size: 24,
                        text_color: TEXT,
                        paragraph: plain_paragraph()
                    )
                    Text (
                        "中文排版 · borrowed MIRX · bounded shaping",
                        font_stack: mixed_stack(),
                        font_size: 13,
                        text_color: MUTED,
                        paragraph: plain_paragraph()
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
                grow: 1.0,
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
                    Text ("ARABIC · RTL JOINING", font: UI, font_size: 12, text_color: VIOLET)
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
                    Text ("DEVANAGARI · CONJUNCTS", font: UI, font_size: 12, text_color: GOLD)
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
                        CaretOverlay (
                            id: "typography_path_carets",
                            target: "typography_path_sample",
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: 190,
                            height: 78
                        )
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
                        ) on Tap { set_path_probe(ctx.world, Point { x: *x, y: *y }); } on DragMove { set_path_probe(ctx.world, Point { x: *x, y: *y }); }
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
                        text: geometry_cost_label(),
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
                height: 154,
                padding: Padding::all(12),
                column_gap: 14,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 14
            ) {
                Column (grow: 1.0, row_gap: 6) {
                    Text ("LIVE PARAGRAPH", font: UI, font_size: 12, text_color: CYAN)
                    View (grow: 1.0, height: 78) {
                        Text (
                            id: "typography_live_sample",
                            LIVE_SAMPLE,
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: ${ sample_width.get().width },
                            height: 78,
                            font_stack: mixed_stack(),
                            font_size: ${ sample_ppem.get().ppem },
                            text_color: TEXT,
                            paragraph: ${ sample_paragraph.get().paragraph() }
                        )
                        CaretOverlay (
                            id: "typography_carets",
                            target: "typography_live_sample",
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: ${ caret_width.get().width },
                            height: 78
                        )
                    }
                    Text (
                        "cyan LTR · violet RTL · lines are authoritative caret stops",
                        font: UI,
                        font_size: 11,
                        text_color: MUTED
                    )
                }
                Column (width: 360, row_gap: 7) {
                    Row (height: 22, align: AlignItems::Center, column_gap: 9) {
                        Text ("PPEM", width: 54, font: UI, font_size: 11, text_color: MUTED)
                        Slider (
                            id: "typography_ppem",
                            grow: 1.0,
                            height: 12,
                            min: Fixed::from_int(10),
                            max: Fixed::from_int(64),
                            value: Fixed::from_int(24),
                            track_color: BORDER,
                            fill_color: CYAN,
                            thumb_color: TEXT
                        ) on ValueChanged {
                            let _ = old;
                            TypographyAction::SetPpem(*new).publish(&ppem_action);
                        }
                        Text (
                            text: ${ format!("{}", ppem_value.get().ppem) },
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
                            value: Fixed::from_int(480),
                            track_color: BORDER,
                            fill_color: BLUE,
                            thumb_color: TEXT
                        ) on ValueChanged {
                            let _ = old;
                            TypographyAction::SetWidth(*new).publish(&width_action);
                        }
                        Text (
                            text: ${ format!("{}", width_value.get().width) },
                            width: 34,
                            font: UI,
                            font_size: 11,
                            text_color: TEXT
                        )
                    }
                    Row (height: 28, column_gap: 7) {
                        Text (
                            id: "typography_wrap",
                            text: ${ wrap_value.get().wrap_label() },
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            font: UI,
                            font_size: 10,
                            text_color: CYAN,
                            paragraph: ParagraphStyle::label()
                        ) on Tap { TypographyAction::CycleWrap.publish(&wrap_action); }
                        Text (
                            id: "typography_align",
                            text: ${ align_value.get().align_label() },
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            font: UI,
                            font_size: 10,
                            text_color: BLUE,
                            paragraph: ParagraphStyle::label()
                        ) on Tap { TypographyAction::CycleAlign.publish(&align_action); }
                        Text (
                            id: "typography_overflow",
                            text: ${ overflow_value.get().overflow_label() },
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            font: UI,
                            font_size: 10,
                            text_color: GOLD,
                            paragraph: ParagraphStyle::label()
                        ) on Tap { TypographyAction::ToggleOverflow.publish(&overflow_action); }
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
    app.with_widget(caret_overlay_view());
    app.with_widget(raster_contour_view());
    register_fonts(&mut app.world);
    let wave_path = register_path(&mut app.world);
    app.compose(parent, |cx| build_widgets(cx, wave_path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::types::Viewport;
    use crate::ui::Parent;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::slider::{SliderEvent, SliderHandler};
    use crate::ui::{IdMap, UiScope};

    fn fixture() -> World {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut views = ViewRegistry::with_builtins();
        views.insert(caret_overlay_view());
        views.insert(raster_contour_view());
        world.insert_resource(views);
        world.insert_resource(crate::render::font::default_font_manager());
        world.insert_resource(crate::text::layout::TextLayoutResource::new(
            crate::text::TextLayoutLimits::HOST,
        ));
        world.insert_resource(crate::render::path::PathStore::new(1).unwrap());
        world.insert_resource(crate::text::baseline::PathBaselineResource::default());
        register_fonts(&mut world);
        let wave_path = register_path(&mut world);
        let parent = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(i32::from(VIEWPORT.0)),
                height: Dimension::px(i32::from(VIEWPORT.1)),
                ..LayoutStyle::default()
            })
            .id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, wave_path);
        drop(cx);
        world
    }

    fn tap(world: &mut World, id: &'static str) {
        let entity = world.find_by_id(id).expect("control id");
        GestureHandler::trigger(
            world,
            entity,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: entity,
            },
        );
        flush_signal_dirty(world);
    }

    #[test]
    fn builds_the_complete_typography_matrix() {
        let world = fixture();

        for id in [
            "typography_latin",
            "typography_cjk",
            "typography_arabic",
            "typography_thai",
            "typography_devanagari",
            "typography_bidi",
            "typography_rasters",
            "typography_path",
            "typography_contour",
            "typography_carets",
            "typography_path_carets",
            "typography_projective_sample",
        ] {
            assert!(world.find_by_id(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn labels_use_text_components() {
        let world = fixture();

        for id in [
            "typography_panel_count",
            "typography_wrap",
            "typography_align",
            "typography_overflow",
        ] {
            let entity = world.find_by_id(id).expect("label id");
            assert!(world.get::<Text>(entity).is_some(), "{id} is not Text");
        }
    }

    #[test]
    fn fallback_stack_stays_borrowed_and_ordered() {
        let stack = mixed_stack();
        assert_eq!(stack.primary(), &UI);
        assert_eq!(stack.fallbacks(), &FALLBACKS);
    }

    #[test]
    fn mixed_bidi_sample_ellipsizes_within_its_card() {
        let mut world = fixture();
        let mut root = world.find_by_id("typography_bidi").unwrap();
        while let Some(parent) = world.get::<Parent>(root).map(|parent| parent.0) {
            root = parent;
        }
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
        );

        let card = world.find_by_id("typography_bidi").unwrap();
        let sample = world.find_by_id("typography_bidi_sample").unwrap();
        let card_rect = world.get::<crate::ui::ComputedRect>(card).unwrap().0;
        let sample_rect = world.get::<crate::ui::ComputedRect>(sample).unwrap().0;
        assert!(sample_rect.x + sample_rect.w <= card_rect.x + card_rect.w);

        let handle = world.get::<crate::text::TextLayoutHandle>(sample).unwrap();
        let layouts = world
            .resource::<crate::text::layout::TextLayoutResource>()
            .unwrap()
            .borrow();
        let layout = layouts.get(*handle).unwrap();
        let text = world.get::<Text>(sample).unwrap().resolve(&world);
        assert_eq!(layout.lines().len(), 1);
        assert!(layout.lines()[0].text().end < text.len() as u32);
        assert!(crate::types::fixed::from_textflow(layout.measure().width) <= sample_rect.w);
        let manager = world.resource::<FontManager>().unwrap();
        let ellipsis = [UI, CJK, ARABIC, DEVANAGARI, THAI, ELLIPSIS]
            .iter()
            .find_map(|token| manager.resolve(token.cache_key()).map_char('…'))
            .expect("ellipsis glyph in the configured font stack");
        assert!(
            layout
                .glyphs()
                .iter()
                .any(|glyph| glyph.glyph_id() == ellipsis)
        );
    }

    #[test]
    fn path_tap_updates_the_caret_probe() {
        let mut world = fixture();
        let sample = world.find_by_id("typography_path_sample").unwrap();
        let overlay = world.find_by_id("typography_path_carets").unwrap();
        assert_eq!(world.get::<CaretOverlay>(overlay).unwrap().probe, None);

        GestureHandler::trigger(
            &mut world,
            sample,
            &GestureEvent::Tap {
                x: Fixed::from_int(42),
                y: Fixed::from_int(55),
                target: sample,
            },
        );
        flush_signal_dirty(&mut world);

        assert_eq!(
            world.get::<CaretOverlay>(overlay).unwrap().probe,
            Some(Point::new(42, 55))
        );
        assert!(world.get::<crate::ui::dirty::Dirty>(overlay).is_some());

        GestureHandler::trigger(
            &mut world,
            sample,
            &GestureEvent::DragMove {
                x: Fixed::from_int(70),
                y: Fixed::from_int(60),
                dx: Fixed::from_int(28),
                dy: Fixed::from_int(5),
                target: sample,
            },
        );
        assert_eq!(
            world.get::<CaretOverlay>(overlay).unwrap().probe,
            Some(Point::new(70, 60))
        );
    }

    #[test]
    fn path_sample_shares_curved_bidi_interaction_geometry() {
        let mut world = fixture();
        let root = world.find_by_id("typography_path").unwrap();
        let mut parent = root;
        while let Some(next) = world.get::<Parent>(parent).map(|parent| parent.0) {
            parent = next;
        }
        crate::ui::render_system::update_layout(
            &mut world,
            parent,
            &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
        );
        let sample = world.find_by_id("typography_path_sample").unwrap();
        let geometry =
            crate::ui::widgets::text::PathTextGeometry::for_widget(&world, sample).unwrap();
        let text = world.get::<Text>(sample).unwrap().resolve(&world);
        let mut storage = [crate::ui::widgets::text::PathSelectionRibbon::default(); 32];
        let ribbons = geometry
            .selection_into(0..text.len() as u32, &mut storage)
            .unwrap();

        assert!(ribbons.iter().any(|ribbon| ribbon.bidi_level() == 0));
        assert!(ribbons.iter().any(|ribbon| ribbon.bidi_level() & 1 == 1));
        assert!(ribbons.windows(2).any(|pair| {
            let first = pair[0].quad();
            let second = pair[1].quad();
            first[1].x - first[0].x != second[1].x - second[0].x
                || first[1].y - first[0].y != second[1].y - second[0].y
        }));

        let ribbon = ribbons[0];
        let quad = ribbon.quad();
        let probe = Point {
            x: (quad[0].x + quad[3].x) / Fixed::from_int(2),
            y: (quad[0].y + quad[3].y) / Fixed::from_int(2),
        };
        let hit = geometry.hit_test(probe, Fixed::ONE).unwrap().unwrap();
        let range = ribbon.text_range();
        assert!(hit.text_offset() == range.start || hit.text_offset() == range.end);
    }

    #[test]
    fn samples_keep_their_shaping_contracts() {
        let world = fixture();

        let arabic = world
            .get::<Text>(world.find_by_id("typography_arabic_sample").unwrap())
            .unwrap();
        assert_eq!(arabic.paragraph().direction, TextDirection::RightToLeft);
        assert_eq!(
            arabic
                .paragraph()
                .language
                .as_ref()
                .map(LanguageTag::as_str),
            Some("ar")
        );

        let thai = world
            .get::<Text>(world.find_by_id("typography_thai_sample").unwrap())
            .unwrap();
        assert_eq!(thai.paragraph().direction, TextDirection::LeftToRight);
        assert_eq!(
            thai.paragraph().language.as_ref().map(LanguageTag::as_str),
            Some("th")
        );

        let devanagari = world
            .get::<Text>(world.find_by_id("typography_devanagari_sample").unwrap())
            .unwrap();
        assert_eq!(devanagari.paragraph().direction, TextDirection::LeftToRight);
        assert_eq!(
            devanagari
                .paragraph()
                .language
                .as_ref()
                .map(LanguageTag::as_str),
            Some("hi")
        );

        let bidi = world
            .get::<crate::ui::Style>(world.find_by_id("typography_bidi_sample").unwrap())
            .unwrap();
        assert_eq!(bidi.font_stack.primary(), &UI);
        assert_eq!(bidi.font_stack.fallbacks(), &FALLBACKS);
    }

    #[test]
    fn controls_publish_into_the_live_paragraph() {
        let mut world = fixture();
        let sample = world
            .find_by_id("typography_live_sample")
            .expect("sample id");
        let ppem = world.find_by_id("typography_ppem").expect("ppem id");
        let width = world.find_by_id("typography_width").expect("width id");

        for (slider, new) in [(ppem, 42), (width, 320)] {
            let callback = world
                .get::<SliderHandler>(slider)
                .expect("slider handler")
                .on_event
                .clone_out();
            callback.call(
                &mut world,
                slider,
                &SliderEvent::ValueChanged {
                    new: Fixed::from_int(new),
                    old: Fixed::ZERO,
                },
            );
        }
        tap(&mut world, "typography_wrap");
        tap(&mut world, "typography_align");
        tap(&mut world, "typography_overflow");

        let style = world.get::<crate::ui::Style>(sample).unwrap();
        assert_eq!(style.font_size, Some(42));
        assert_eq!(style.layout.width, Dimension::px(320));
        let paragraph = world.get::<Text>(sample).unwrap().paragraph();
        assert_eq!(paragraph.wrap, TextWrap::Grapheme);
        assert_eq!(paragraph.align, TextAlign::Center);
        assert_eq!(paragraph.overflow, TextOverflow::Ellipsis);
        assert_eq!(paragraph.max_lines, Some(2));
    }
}
