extern crate alloc;

use alloc::format;

use crate::prelude::*;
use crate::render::font::{Font, FontManager, FontStack};
use crate::ui::widgets::text::FontFeature;
use crate::ui::widgets::{
    FontFeatures, LanguageTag, ParagraphStyle, ShapingPolicy, Slider, Text, TextAlign,
    TextDirection, TextOverflow, TextWrap,
};

pub const VIEWPORT: (u16, u16) = (1024, 720);

const UI_FONT: &[u8] = include_bytes!("assets/misans_ui.mirx");
const CJK_FONT: &[u8] = include_bytes!("assets/typography_cjk.mirx");
const ARABIC_FONT: &[u8] = include_bytes!("assets/typography_arabic.mirx");
const THAI_FONT: &[u8] = include_bytes!("assets/typography_thai.mirx");

const UI: FontToken = FontToken::Custom("typography_ui");
const CJK: FontToken = FontToken::Custom("typography_cjk");
const ARABIC: FontToken = FontToken::Custom("typography_arabic");
const THAI: FontToken = FontToken::Custom("typography_thai");
const FALLBACKS: [FontToken; 3] = [CJK, ARABIC, THAI];
const FEATURES_OFF: [FontFeature; 2] =
    [FontFeature::new(*b"liga", 0), FontFeature::new(*b"kern", 0)];

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

const BACKGROUND: Color = Color::rgb(10, 17, 29);
const PANEL: Color = Color::rgb(17, 29, 48);
const PANEL_ALT: Color = Color::rgb(21, 36, 58);
const BORDER: Color = Color::rgb(46, 70, 98);
const TEXT: Color = Color::rgb(231, 239, 248);
const MUTED: Color = Color::rgb(143, 164, 188);
const CYAN: Color = Color::rgb(82, 221, 207);
const BLUE: Color = Color::rgb(104, 161, 255);
const GOLD: Color = Color::rgb(255, 197, 92);
const VIOLET: Color = Color::rgb(177, 132, 255);

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
    manager.add_static(THAI.cache_key(), font(THAI_FONT, "Noto Sans Thai"));
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

fn features_off() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        max_lines: Some(1),
        features: FontFeatures::borrowed(&FEATURES_OFF).expect("bounded features"),
        ..ParagraphStyle::default()
    }
}

#[compose]
pub fn build_widgets() {
    let state = Signal::new(TypographyState::default());
    let sample_width = state.clone();
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
                View (
                    width: 158,
                    height: 30,
                    bg_color: PANEL_ALT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 15,
                    text: "6 TEST PANELS",
                    font: UI,
                    font_size: 12,
                    text_color: CYAN
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
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 8,
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("LATIN · GSUB / GPOS", font: UI, font_size: 12, text_color: BLUE)
                    Text (
                        id: "typography_latin_shaped",
                        "office ffi · AVATAR To",
                        font: UI,
                        font_size: 27,
                        text_color: TEXT,
                        paragraph: plain_paragraph()
                    )
                    Text (
                        id: "typography_latin_plain",
                        "office ffi · AVATAR To",
                        font: UI,
                        font_size: 17,
                        text_color: MUTED,
                        paragraph: features_off()
                    )
                    Text (
                        "top liga kern · bottom disabled",
                        font: UI,
                        font_size: 12,
                        text_color: MUTED
                    )
                }
                Column (
                    id: "typography_cjk",
                    grow: 1.0,
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 9,
                    bg_color: PANEL_ALT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("CJK · GLYPH METRICS", font: UI, font_size: 12, text_color: GOLD)
                    Text (
                        id: "typography_cjk_sample",
                        "中文字体排版",
                        font: CJK,
                        font_size: 30,
                        text_color: TEXT,
                        paragraph: plain_paragraph()
                    )
                    Text (
                        "真实 bearing · advance · atlas bounds",
                        font: UI,
                        font_size: 13,
                        text_color: MUTED
                    )
                }
                Column (
                    id: "typography_arabic",
                    grow: 1.0,
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 9,
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("ARABIC · RTL JOINING", font: UI, font_size: 12, text_color: VIOLET)
                    Text (
                        id: "typography_arabic_sample",
                        "مَرْحَبًا بِالْعَالَمِ",
                        font: ARABIC,
                        font_size: 30,
                        text_color: TEXT,
                        paragraph: paragraph(Some("ar"), TextDirection::RightToLeft)
                    )
                    Text (
                        "joining · cursive · mark anchors",
                        font: UI,
                        font_size: 13,
                        text_color: MUTED
                    )
                }
                Column (
                    id: "typography_thai",
                    grow: 1.0,
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 9,
                    bg_color: PANEL_ALT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("THAI · MARK PLACEMENT", font: UI, font_size: 12, text_color: CYAN)
                    Text (
                        id: "typography_thai_sample",
                        "สวัสดีครับ ภาษาไทย",
                        font: THAI,
                        font_size: 27,
                        text_color: TEXT,
                        paragraph: paragraph(Some("th"), TextDirection::LeftToRight)
                    )
                    Text (
                        "decomposition · GDEF mark filtering",
                        font: UI,
                        font_size: 13,
                        text_color: MUTED
                    )
                }
                Column (
                    id: "typography_bidi",
                    grow: 1.0,
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 9,
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("MIXED BIDI · FALLBACK", font: UI, font_size: 12, text_color: BLUE)
                    Text (
                        id: "typography_bidi_sample",
                        "mirui 42 · 中文字体排版 · مرحبا",
                        font_stack: mixed_stack(),
                        font_size: 21,
                        text_color: TEXT,
                        paragraph: paragraph(None, TextDirection::Auto)
                    )
                    Text (
                        "grapheme-safe face selection",
                        font: UI,
                        font_size: 13,
                        text_color: MUTED
                    )
                }
                Column (
                    id: "typography_rasters",
                    grow: 1.0,
                    min_width: 300,
                    height: 186,
                    padding: Padding::all(14),
                    row_gap: 7,
                    bg_color: PANEL_ALT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14
                ) {
                    Text ("COVERAGE / SDF", font: UI, font_size: 12, text_color: GOLD)
                    Row (height: 66, align: AlignItems::Center, column_gap: 12) {
                        Text ("Aa", font: UI, font_size: 11, text_color: MUTED)
                        Text ("Aa", font: UI, font_size: 22, text_color: TEXT)
                        Text ("Aa", font: UI, font_size: 38, text_color: CYAN)
                        Text ("Aa", font: UI, font_size: 56, text_color: BLUE)
                    }
                    Text (
                        "exact A8 sizes → bounded distance field",
                        font: UI,
                        font_size: 13,
                        text_color: MUTED
                    )
                    Text (
                        "missing scalar 10FFFF uses .notdef",
                        font: UI,
                        font_size: 12,
                        text_color: VIOLET,
                        paragraph: plain_paragraph()
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
                    Text (
                        id: "typography_live_sample",
                        "office AVATAR · 中文字体排版 · مرحبا بالعالم · ภาษาไทย",
                        width: ${ sample_width.get().width },
                        height: 78,
                        font_stack: mixed_stack(),
                        font_size: ${ sample_ppem.get().ppem },
                        text_color: TEXT,
                        paragraph: ${ sample_paragraph.get().paragraph() }
                    )
                    Text (
                        "Signal state → paragraph layout → shared positioned glyphs",
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
                        View (
                            id: "typography_wrap",
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            text: ${ wrap_value.get().wrap_label() },
                            font: UI,
                            font_size: 10,
                            text_color: CYAN
                        ) on Tap { TypographyAction::CycleWrap.publish(&wrap_action); }
                        View (
                            id: "typography_align",
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            text: ${ align_value.get().align_label() },
                            font: UI,
                            font_size: 10,
                            text_color: BLUE
                        ) on Tap { TypographyAction::CycleAlign.publish(&align_action); }
                        View (
                            id: "typography_overflow",
                            grow: 1.0,
                            height: 28,
                            bg_color: PANEL_ALT,
                            border_color: BORDER,
                            border_width: 1,
                            border_radius: 8,
                            text: ${ overflow_value.get().overflow_label() },
                            font: UI,
                            font_size: 10,
                            text_color: GOLD
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
    register_fonts(&mut app.world);
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::widgets::slider::{SliderEvent, SliderHandler};
    use crate::ui::{IdMap, UiScope};

    fn fixture() -> World {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        register_fonts(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
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
            "typography_bidi",
            "typography_rasters",
        ] {
            assert!(world.find_by_id(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn fallback_stack_stays_borrowed_and_ordered() {
        let stack = mixed_stack();
        assert_eq!(stack.primary(), &UI);
        assert_eq!(stack.fallbacks(), &FALLBACKS);
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
