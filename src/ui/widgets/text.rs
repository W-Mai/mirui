use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::core::i18n::Localized;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect};
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
    let Some(text) = world.get::<Text>(entity) else {
        return;
    };
    let color = ctx.style.text_color.resolve_in(ctx.theme(world), ctx.state);
    let Some(font) = crate::render::font::resolve_or_default(world, ctx.style.font_stack.primary())
    else {
        return;
    };
    let s = text.resolve(world);
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(2),
            },
            transform: ctx.transform,
            text: &s,
            font: &font,
            color,
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("Text", 80, text_render).with_filter::<Text>()
}

#[cfg(test)]
mod tests {
    use super::*;

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
