use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::core::i18n::Localized;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect, Transform};
use crate::ui::view::{View, ViewCtx};

pub use textflow::shaping::FontFeature;

pub const MAX_FONT_FEATURES: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextWrap {
    NoWrap,
    #[default]
    Word,
    Grapheme,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ShapingPolicy {
    #[default]
    Auto,
    Required,
    Simple,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LanguageTag(Cow<'static, str>);

impl LanguageTag {
    pub fn parse(tag: impl Into<Cow<'static, str>>) -> Result<Self, LanguageTagError> {
        let tag = tag.into();
        validate_language_tag(&tag)?;
        Ok(Self(tag))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguageTagError {
    Empty,
    EmptySubtag,
    InvalidByte { index: usize },
}

fn validate_language_tag(tag: &str) -> Result<(), LanguageTagError> {
    if tag.is_empty() {
        return Err(LanguageTagError::Empty);
    }
    let bytes = tag.as_bytes();
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return Err(LanguageTagError::EmptySubtag);
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == b'-' {
            if bytes[index - 1] == b'-' {
                return Err(LanguageTagError::EmptySubtag);
            }
        } else if !byte.is_ascii_alphanumeric() {
            return Err(LanguageTagError::InvalidByte { index });
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontFeatures(Cow<'static, [FontFeature]>);

impl FontFeatures {
    pub const fn new() -> Self {
        Self(Cow::Borrowed(&[]))
    }

    pub fn borrowed(features: &'static [FontFeature]) -> Result<Self, FontFeatureCapacityError> {
        Self::validate(features.len())?;
        Ok(Self(Cow::Borrowed(features)))
    }

    pub fn owned(features: Vec<FontFeature>) -> Result<Self, FontFeatureCapacityError> {
        Self::validate(features.len())?;
        Ok(Self(Cow::Owned(features)))
    }

    pub fn as_slice(&self) -> &[FontFeature] {
        &self.0
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self.0, Cow::Borrowed(_))
    }

    fn validate(count: usize) -> Result<(), FontFeatureCapacityError> {
        if count > MAX_FONT_FEATURES {
            Err(FontFeatureCapacityError { count })
        } else {
            Ok(())
        }
    }
}

impl Default for FontFeatures {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontFeatureCapacityError {
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParagraphStyle {
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub overflow: TextOverflow,
    pub max_lines: Option<u16>,
    pub line_height: Option<Fixed>,
    pub letter_spacing: Fixed,
    pub word_spacing: Fixed,
    pub direction: TextDirection,
    pub language: Option<LanguageTag>,
    pub shaping: ShapingPolicy,
    pub features: FontFeatures,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            wrap: TextWrap::Word,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
            max_lines: None,
            line_height: None,
            letter_spacing: Fixed::ZERO,
            word_spacing: Fixed::ZERO,
            direction: TextDirection::Auto,
            language: None,
            shaping: ShapingPolicy::Auto,
            features: FontFeatures::new(),
        }
    }
}

impl ParagraphStyle {
    pub(crate) fn layout_request<'a>(
        &'a self,
        text: &'a str,
        metrics: crate::render::font::FontMetrics,
        width: Option<Fixed>,
    ) -> crate::text::layout::TextLayoutRequest<'a> {
        use textflow::bidi::BaseDirection;
        use textflow::layout::{Alignment, Overflow, TextSpacing, WrapMode};

        use crate::types::fixed::to_textflow;

        let direction = match self.direction {
            TextDirection::Auto => BaseDirection::Auto,
            TextDirection::LeftToRight => BaseDirection::LeftToRight,
            TextDirection::RightToLeft => BaseDirection::RightToLeft,
        };
        let max_width = match (self.wrap, width) {
            (TextWrap::NoWrap, _) | (_, None) => i32::MAX,
            (TextWrap::Word | TextWrap::Grapheme, Some(width)) => {
                to_textflow(width.max(Fixed::ZERO))
            }
        };
        let wrap = match self.wrap {
            TextWrap::NoWrap => WrapMode::NoWrap,
            TextWrap::Word => WrapMode::WordOrGrapheme,
            TextWrap::Grapheme => WrapMode::Grapheme,
        };
        let alignment = match self.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        let overflow = match self.overflow {
            TextOverflow::Clip => Overflow::Clip,
            TextOverflow::Ellipsis => Overflow::Ellipsis,
        };
        crate::text::layout::TextLayoutRequest {
            text,
            max_width,
            width: width.map(|width| to_textflow(width.max(Fixed::ZERO))),
            max_lines: self.max_lines.map(usize::from).unwrap_or(usize::MAX),
            line_height: to_textflow(self.line_height.unwrap_or(metrics.line_height)),
            baseline: to_textflow(metrics.ascender),
            direction,
            wrap,
            alignment,
            overflow,
            spacing: TextSpacing {
                letter: to_textflow(self.letter_spacing),
                word: to_textflow(self.word_spacing),
            },
            features: self.features.as_slice(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TextContent {
    Plain(Cow<'static, str>),
    Localized(Localized),
}

impl TextContent {
    fn resolve<'a>(&'a self, world: &World) -> Cow<'a, str> {
        match self {
            Self::Plain(content) => Cow::Borrowed(content.as_ref()),
            Self::Localized(localized) => Cow::Borrowed(localized.resolve_or_key(world)),
        }
    }

    fn is_localized(&self) -> bool {
        matches!(self, Self::Localized(_))
    }
}

#[derive(Clone, Debug, crate::Component)]
pub struct Text {
    content: TextContent,
    paragraph: ParagraphStyle,
}

impl Text {
    pub fn new(content: impl Into<TextContent>) -> Self {
        Self {
            content: content.into(),
            paragraph: ParagraphStyle::default(),
        }
    }

    pub fn build(source: impl Into<Text>) -> TextBuilder {
        TextBuilder {
            text: source.into(),
            style: None,
        }
    }

    pub fn resolve<'a>(&'a self, world: &World) -> Cow<'a, str> {
        self.content.resolve(world)
    }

    pub fn is_localized(&self) -> bool {
        self.content.is_localized()
    }

    pub fn content(&self) -> &TextContent {
        &self.content
    }

    pub fn paragraph(&self) -> &ParagraphStyle {
        &self.paragraph
    }

    pub fn set_content(&mut self, content: impl Into<TextContent>) -> &mut Self {
        self.content = content.into();
        self
    }

    pub fn set_paragraph(&mut self, paragraph: ParagraphStyle) -> &mut Self {
        self.paragraph = paragraph;
        self
    }

    pub fn with_paragraph(mut self, paragraph: ParagraphStyle) -> Self {
        self.paragraph = paragraph;
        self
    }
}

impl From<&'static str> for TextContent {
    fn from(s: &'static str) -> Self {
        Self::Plain(Cow::Borrowed(s))
    }
}

impl From<String> for TextContent {
    fn from(s: String) -> Self {
        Self::Plain(Cow::Owned(s))
    }
}

impl From<Cow<'static, str>> for TextContent {
    fn from(c: Cow<'static, str>) -> Self {
        Self::Plain(c)
    }
}

impl From<Localized> for TextContent {
    fn from(loc: Localized) -> Self {
        Self::Localized(loc)
    }
}

impl From<TextContent> for Text {
    fn from(content: TextContent) -> Self {
        Self::new(content)
    }
}

impl From<&'static str> for Text {
    fn from(content: &'static str) -> Self {
        Self::new(content)
    }
}

impl From<String> for Text {
    fn from(content: String) -> Self {
        Self::new(content)
    }
}

impl From<Cow<'static, str>> for Text {
    fn from(content: Cow<'static, str>) -> Self {
        Self::new(content)
    }
}

impl From<Localized> for Text {
    fn from(content: Localized) -> Self {
        Self::new(content)
    }
}

pub struct TextBuilder {
    text: Text,
    style: Option<crate::ui::Style>,
}

impl TextBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn paragraph(mut self, paragraph: ParagraphStyle) -> Self {
        self.text.paragraph = paragraph;
        self
    }

    pub fn font(mut self, token: impl Into<crate::render::font::FontToken>) -> Self {
        self.style.get_or_insert_default().set_font_token(token);
        self
    }

    pub fn font_stack(mut self, stack: impl Into<crate::render::font::FontStack>) -> Self {
        self.style.get_or_insert_default().set_font_stack(stack);
        self
    }

    pub fn font_size(mut self, size: u16) -> Self {
        self.style.get_or_insert_default().set_font_size(size);
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for TextBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.text);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
    }
}

fn text_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if world.get::<Text>(entity).is_none() {
        return;
    }
    let color = ctx.style.text_color.resolve_in(ctx.theme(world), ctx.state);
    let Some(resource) = world.resource::<crate::text::layout::TextLayoutResource>() else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let Ok(Some(fonts)) = crate::render::font::ResolvedFontStack::resolve(
        world,
        &ctx.style.font_stack,
        ctx.style.font_size,
        face_limit,
    ) else {
        return;
    };
    let Some(handle) = world.get::<crate::text::TextLayoutHandle>(entity).copied() else {
        return;
    };
    let cache = resource.borrow();
    let Some(layout) = cache.get(handle) else {
        return;
    };
    draw_text_layout(
        renderer,
        &layout,
        |font_id| fonts.font(font_id),
        TextPaint::new(
            Point {
                x: rect.x,
                y: rect.y,
            },
            ctx.transform,
            ctx.clip,
            color,
        ),
    );
}

pub(crate) struct TextPaint<'a> {
    origin: Point,
    transform: Transform,
    clip: &'a Rect,
    color: crate::types::Color,
}

impl<'a> TextPaint<'a> {
    pub(crate) const fn new(
        origin: Point,
        transform: Transform,
        clip: &'a Rect,
        color: crate::types::Color,
    ) -> Self {
        Self {
            origin,
            transform,
            clip,
            color,
        }
    }
}

pub(crate) fn draw_text_layout<'font>(
    renderer: &mut dyn Renderer,
    layout: &crate::text::TextLayout<'_>,
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    paint: TextPaint<'_>,
) {
    for line in layout.lines() {
        let Some(runs) = layout.runs_for(*line) else {
            continue;
        };
        for run in runs {
            let Some(font) = font_for(run.font_id()) else {
                continue;
            };
            let Some(glyph_origin) = layout
                .glyphs_for(*run)
                .and_then(|glyphs| glyphs.first())
                .map(|glyph| glyph.origin)
            else {
                continue;
            };
            let metrics = font.metrics(font.size);
            renderer.draw(
                &DrawCommand::GlyphRun {
                    pos: Point {
                        x: paint.origin.x + crate::types::fixed::from_textflow(glyph_origin.x),
                        y: paint.origin.y + crate::types::fixed::from_textflow(glyph_origin.y)
                            - metrics.ascender,
                    },
                    transform: paint.transform,
                    glyphs: layout.glyphs_for(*run).unwrap_or_default(),
                    font,
                    color: paint.color,
                    opa: 255,
                },
                paint.clip,
            );
        }
    }
}

/// Borrowed positioned glyphs for rendering without runtime shaping or allocation.
#[derive(Clone, Copy, Debug, crate::Component)]
pub struct StaticGlyphRun {
    glyphs: &'static [textflow::shaping::PositionedGlyph],
}

impl StaticGlyphRun {
    pub const fn new(glyphs: &'static [textflow::shaping::PositionedGlyph]) -> Self {
        Self { glyphs }
    }

    pub const fn glyphs(&self) -> &'static [textflow::shaping::PositionedGlyph] {
        self.glyphs
    }

    pub(crate) fn measure(&self, line_height: Fixed) -> (Fixed, Fixed) {
        let Some(first) = self.glyphs.first() else {
            return (Fixed::ZERO, Fixed::ZERO);
        };
        let mut right = 0;
        let mut min_baseline = first.origin.y;
        let mut max_baseline = first.origin.y;
        for glyph in self.glyphs {
            right = right.max(
                glyph
                    .origin
                    .x
                    .saturating_add(glyph.offset.x)
                    .saturating_add(glyph.advance.x),
            );
            min_baseline = min_baseline.min(glyph.origin.y.saturating_add(glyph.offset.y));
            max_baseline = max_baseline.max(glyph.origin.y.saturating_add(glyph.offset.y));
        }
        (
            crate::types::fixed::from_textflow(right.max(0)),
            line_height
                + crate::types::fixed::from_textflow(max_baseline.saturating_sub(min_baseline)),
        )
    }
}

fn static_glyph_run_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(run) = world.get::<StaticGlyphRun>(entity) else {
        return;
    };
    let Some(first) = run.glyphs.first() else {
        return;
    };
    let Some(font) = crate::render::font::resolve_or_default(world, ctx.style.font_stack.primary())
    else {
        return;
    };
    let mut font = font.as_ref().clone();
    font.size = ctx.style.font_size.unwrap_or(font.size).max(1);
    let metrics = font.metrics(font.size);
    renderer.draw(
        &DrawCommand::GlyphRun {
            pos: Point {
                x: rect.x + crate::types::fixed::from_textflow(first.origin.x),
                y: rect.y + crate::types::fixed::from_textflow(first.origin.y) - metrics.ascender,
            },
            transform: ctx.transform,
            glyphs: run.glyphs,
            font: &font,
            color: ctx.style.text_color.resolve_in(ctx.theme(world), ctx.state),
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("Text", 80, text_render).with_filter::<Text>()
}

pub fn static_glyph_run_view() -> View {
    View::new("StaticGlyphRun", 80, static_glyph_run_render).with_filter::<StaticGlyphRun>()
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::render::font::{GlyphId, default_font_manager};
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::theme::{Theme, WidgetState};
    use textflow::shaping::{FlowPoint, PositionedGlyph};

    #[derive(Default)]
    struct RecordingRenderer {
        glyph_runs: Vec<(Point, usize)>,
    }

    impl Renderer for RecordingRenderer {
        fn draw(&mut self, command: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::GlyphRun { pos, glyphs, .. } = command {
                self.glyph_runs.push((*pos, glyphs.len()));
            }
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn static_run_renders_without_a_layout_workspace() {
        static GLYPHS: [PositionedGlyph; 2] = [
            PositionedGlyph::new(GlyphId::new(65), FlowPoint { x: 0, y: 7 << 8 })
                .with_advance(FlowPoint { x: 8 << 8, y: 0 }),
            PositionedGlyph::new(
                GlyphId::new(66),
                FlowPoint {
                    x: 8 << 8,
                    y: 7 << 8,
                },
            )
            .with_advance(FlowPoint { x: 8 << 8, y: 0 }),
        ];
        let mut world = World::new();
        world.insert_resource(Theme::default());
        world.insert_resource(default_font_manager());
        let entity = world.spawn_empty();
        world.insert(entity, StaticGlyphRun::new(&GLYPHS));
        let style = Style::default();
        let rect = Rect {
            x: Fixed::from_int(3),
            y: Fixed::from_int(5),
            w: Fixed::from_int(20),
            h: Fixed::from_int(10),
        };
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::IDENTITY,
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
        };
        let mut renderer = RecordingRenderer::default();

        static_glyph_run_render(&mut renderer, &world, entity, &rect, &mut ctx);

        assert_eq!(renderer.glyph_runs, vec![(Point::new(3, 5), 2)]);
        assert_eq!(
            StaticGlyphRun::new(&GLYPHS).measure(Fixed::from_int(8)),
            (Fixed::from_int(16), Fixed::from_int(8))
        );
    }

    #[test]
    fn build_spawns_text_with_style() {
        let mut world = World::new();
        let e = Text::build("hi")
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        let text = world.get::<Text>(e).unwrap();
        assert_eq!(text.resolve(&world), "hi");
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Text::build("hi").spawn(&mut world);
        assert!(world.has::<Text>(e));
        assert!(!world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn static_str_is_borrowed() {
        let t: Text = "hi".into();
        match t.content() {
            TextContent::Plain(Cow::Borrowed(s)) => assert_eq!(*s, "hi"),
            _ => panic!("expected borrowed cow for &'static str"),
        }
    }

    #[test]
    fn string_is_owned() {
        let s: String = "hi".into();
        let t: Text = s.into();
        assert!(matches!(t.content(), TextContent::Plain(Cow::Owned(_))));
    }

    #[test]
    fn from_localized_yields_localized() {
        let t: Text = Localized::new("welcome").into();
        assert!(t.is_localized());
    }

    #[test]
    fn paragraph_defaults_are_layout_safe() {
        let paragraph = ParagraphStyle::default();
        assert_eq!(paragraph.wrap, TextWrap::Word);
        assert_eq!(paragraph.align, TextAlign::Start);
        assert_eq!(paragraph.shaping, ShapingPolicy::Auto);
        assert!(paragraph.features.as_slice().is_empty());

        let request = paragraph.layout_request(
            "unbroken",
            crate::render::font::FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-1),
                line_height: Fixed::from_int(8),
            },
            Some(Fixed::from_int(4)),
        );
        assert_eq!(request.wrap, textflow::layout::WrapMode::WordOrGrapheme);
    }

    #[test]
    fn paragraph_request_preserves_overflow_policy() {
        let paragraph = ParagraphStyle {
            overflow: TextOverflow::Ellipsis,
            max_lines: Some(2),
            ..ParagraphStyle::default()
        };
        let request = paragraph.layout_request(
            "overflow",
            crate::render::font::FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-1),
                line_height: Fixed::from_int(8),
            },
            Some(Fixed::from_int(24)),
        );

        assert_eq!(request.overflow, textflow::layout::Overflow::Ellipsis);
        assert_eq!(request.max_lines, 2);
    }

    #[test]
    fn feature_storage_is_bounded() {
        static FEATURES: [FontFeature; MAX_FONT_FEATURES] =
            [FontFeature::new(*b"kern", 1); MAX_FONT_FEATURES];
        let features = FontFeatures::borrowed(&FEATURES).unwrap();
        assert_eq!(features.as_slice().len(), MAX_FONT_FEATURES);
        assert!(features.is_borrowed());
        assert_eq!(
            FontFeatures::owned(alloc::vec![
                FontFeature::new(*b"liga", 1);
                MAX_FONT_FEATURES + 1
            ]),
            Err(FontFeatureCapacityError {
                count: MAX_FONT_FEATURES + 1
            })
        );
    }

    #[test]
    fn language_tags_reject_ambiguous_separators() {
        assert_eq!(
            LanguageTag::parse("zh--Hans"),
            Err(LanguageTagError::EmptySubtag)
        );
        assert_eq!(LanguageTag::parse("ar-EG").unwrap().as_str(), "ar-EG");
    }
}
