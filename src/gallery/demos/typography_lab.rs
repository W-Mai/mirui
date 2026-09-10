extern crate alloc;

use crate::prelude::*;
use crate::render::font::{Font, FontManager, FontStack};
use crate::ui::widgets::text::FontFeature;
use crate::ui::widgets::{
    FontFeatures, LanguageTag, ParagraphStyle, ShapingPolicy, Text, TextDirection, TextWrap,
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
    use crate::ui::{IdMap, UiScope};

    #[test]
    fn builds_the_complete_typography_matrix() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        register_fonts(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);

        build_widgets(&mut cx);

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
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        register_fonts(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);

        build_widgets(&mut cx);

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
}
