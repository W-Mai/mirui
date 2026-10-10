use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::core::i18n::{I18n, Localized};
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::text::baseline::PlacementKey;
use crate::types::{Fixed, Fixed64, Point, Rect, Transform, Transform3D};
use crate::ui::view::PaintCtx;

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
pub enum TextVerticalAlign {
    #[default]
    Start,
    Center,
    End,
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
    pub vertical_align: TextVerticalAlign,
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
            vertical_align: TextVerticalAlign::Start,
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
    /// A single-line paragraph centered within a control.
    pub fn label() -> Self {
        Self {
            wrap: TextWrap::NoWrap,
            align: TextAlign::Center,
            vertical_align: TextVerticalAlign::Center,
            ..Self::default()
        }
    }

    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_vertical_align(mut self, align: TextVerticalAlign) -> Self {
        self.vertical_align = align;
        self
    }

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
            line_widths: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextContent {
    Plain(Cow<'static, str>),
    Localized(Localized),
}

impl TextContent {
    fn resolve(&self, i18n: Option<&I18n>) -> Cow<'_, str> {
        match self {
            Self::Plain(content) => Cow::Borrowed(content.as_ref()),
            Self::Localized(localized) => Cow::Borrowed(localized.resolve_or_key(i18n)),
        }
    }

    fn is_localized(&self) -> bool {
        matches!(self, Self::Localized(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextContentError {
    Uninitialized,
    Capacity { required: usize, capacity: usize },
    CapacityMismatch { declared: usize, provided: usize },
    Format,
    Localized,
    Unbounded,
    ReserveFailed { capacity: usize },
}

impl fmt::Display for TextContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninitialized => formatter.write_str("bounded text has no initial value"),
            Self::Capacity { required, capacity } => {
                write!(
                    formatter,
                    "text requires {required} UTF-8 bytes, capacity is {capacity}"
                )
            }
            Self::CapacityMismatch { declared, provided } => write!(
                formatter,
                "text capacity is {declared} UTF-8 bytes, formatter uses {provided}"
            ),
            Self::Format => formatter.write_str("text formatting failed"),
            Self::Localized => formatter.write_str("localized content needs bounded formatting"),
            Self::Unbounded => formatter.write_str("text has no declared capacity"),
            Self::ReserveFailed { capacity } => {
                write!(
                    formatter,
                    "could not reserve {capacity} UTF-8 bytes for text"
                )
            }
        }
    }
}

struct BoundedTextBuffer<const N: usize> {
    bytes: [u8; N],
    len: usize,
    required: usize,
}

impl<const N: usize> BoundedTextBuffer<N> {
    fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            required: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("formatter wrote UTF-8")
    }
}

impl<const N: usize> fmt::Write for BoundedTextBuffer<N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.required = self.required.saturating_add(value.len());
        if self.required <= N {
            self.bytes[self.len..self.required].copy_from_slice(value.as_bytes());
            self.len = self.required;
        }
        Ok(())
    }
}

#[crate::component]
#[derive(Debug)]
pub struct Text {
    content: TextContent,
    paragraph: ParagraphStyle,
    text_capacity: Option<usize>,
    last_content_error: Option<TextContentError>,
    has_valid_content: bool,
}

impl Clone for Text {
    fn clone(&self) -> Self {
        let content = match (&self.content, self.text_capacity) {
            (TextContent::Plain(value), Some(capacity)) => {
                let mut storage = String::with_capacity(capacity);
                storage.push_str(value);
                TextContent::Plain(Cow::Owned(storage))
            }
            _ => self.content.clone(),
        };
        Self {
            content,
            paragraph: self.paragraph.clone(),
            text_capacity: self.text_capacity,
            last_content_error: self.last_content_error,
            has_valid_content: self.has_valid_content,
        }
    }
}

impl Text {
    pub fn new(content: impl Into<TextContent>) -> Self {
        Self {
            content: content.into(),
            paragraph: ParagraphStyle::default(),
            text_capacity: None,
            last_content_error: None,
            has_valid_content: true,
        }
    }

    pub fn new_bounded(capacity: usize) -> Result<Self, TextContentError> {
        let mut text = Self::new("").with_text_capacity(capacity)?;
        text.has_valid_content = false;
        text.last_content_error = Some(TextContentError::Uninitialized);
        Ok(text)
    }

    pub fn with_text_capacity(mut self, capacity: usize) -> Result<Self, TextContentError> {
        let already_bounded = self.text_capacity.is_some();
        let TextContent::Plain(value) = &self.content else {
            return Err(TextContentError::Localized);
        };
        if value.len() > capacity {
            return Err(TextContentError::Capacity {
                required: value.len(),
                capacity,
            });
        }
        let mut storage = String::new();
        storage
            .try_reserve_exact(capacity)
            .map_err(|_| TextContentError::ReserveFailed { capacity })?;
        storage.push_str(value);
        self.content = TextContent::Plain(Cow::Owned(storage));
        self.text_capacity = Some(capacity);
        if !already_bounded {
            self.last_content_error = None;
            self.has_valid_content = true;
        }
        Ok(self)
    }

    /// Creates a single-line label centered within its layout bounds.
    pub fn label(content: impl Into<TextContent>) -> Self {
        Self::new(content).with_paragraph(ParagraphStyle::label())
    }

    pub fn build(source: impl Into<Text>) -> TextBuilder {
        TextBuilder {
            text: source.into(),
            style: None,
            path: None,
            properties: (),
            bindings: (),
        }
    }

    /// Borrow the current text, resolving localized keys with the supplied translation resource. Missing translations fall back to the key. Typed callbacks can pass their `res!(optional I18n)` parameter directly; plain content needs no translation resource.
    pub fn resolve(&self, i18n: Option<&I18n>) -> Cow<'_, str> {
        self.content.resolve(i18n)
    }

    pub fn is_localized(&self) -> bool {
        self.content.is_localized()
    }

    pub fn content(&self) -> &TextContent {
        &self.content
    }

    pub fn text_capacity(&self) -> Option<usize> {
        self.text_capacity
    }

    pub fn last_content_error(&self) -> Option<TextContentError> {
        self.last_content_error
    }

    pub fn has_valid_content(&self) -> bool {
        self.has_valid_content
    }

    pub fn paragraph(&self) -> &ParagraphStyle {
        &self.paragraph
    }

    pub fn set_content(&mut self, content: impl Into<TextContent>) -> &mut Self {
        self.try_set_content(content)
            .expect("text content exceeds its declared capacity");
        self
    }

    pub fn try_set_content(
        &mut self,
        content: impl Into<TextContent>,
    ) -> Result<bool, TextContentError> {
        let content = content.into();
        if let Some(capacity) = self.text_capacity {
            let TextContent::Plain(value) = content else {
                return self.content_failure(TextContentError::Localized);
            };
            if value.len() > capacity {
                return self.content_failure(TextContentError::Capacity {
                    required: value.len(),
                    capacity,
                });
            }
            return Ok(self.commit_bounded_content(value.as_ref()));
        }
        let changed = self.content != content;
        if changed {
            self.content = content;
        }
        self.last_content_error = None;
        self.has_valid_content = true;
        Ok(changed)
    }

    pub fn try_set_bounded<const N: usize>(
        &mut self,
        args: fmt::Arguments<'_>,
    ) -> Result<bool, TextContentError> {
        let Some(capacity) = self.text_capacity else {
            return self.content_failure(TextContentError::Unbounded);
        };
        if N != capacity {
            return self.content_failure(TextContentError::CapacityMismatch {
                declared: capacity,
                provided: N,
            });
        }
        let mut buffer = BoundedTextBuffer::<N>::new();
        if fmt::write(&mut buffer, args).is_err() {
            return self.content_failure(TextContentError::Format);
        }
        if buffer.required > capacity {
            return self.content_failure(TextContentError::Capacity {
                required: buffer.required,
                capacity,
            });
        }
        Ok(self.commit_bounded_content(buffer.as_str()))
    }

    fn commit_bounded_content(&mut self, value: &str) -> bool {
        let TextContent::Plain(Cow::Owned(current)) = &mut self.content else {
            unreachable!("bounded text owns its content storage");
        };
        let changed = !self.has_valid_content || current != value;
        if changed {
            current.clear();
            current.push_str(value);
        }
        self.last_content_error = None;
        self.has_valid_content = true;
        changed
    }

    fn content_failure<T>(&mut self, error: TextContentError) -> Result<T, TextContentError> {
        self.last_content_error = Some(error);
        Err(error)
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

pub struct TextBuilder<P = (), B = ()> {
    text: Text,
    style: Option<crate::ui::Style>,
    path: Option<crate::text::TextPath>,
    properties: P,
    bindings: B,
}

impl<P, B> TextBuilder<P, B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn paragraph(mut self, paragraph: ParagraphStyle) -> Self {
        self.text.paragraph = paragraph;
        self
    }

    pub fn path(mut self, path: impl Into<crate::text::TextPath>) -> Self {
        self.path = Some(path.into());
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
}

crate::ui::recipe::widget_builder_bindings!(TextBuilder, Text, text, style, path);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for TextBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.text);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(path) = self.path {
            world.insert(entity, path);
        }
        (self.properties, self.bindings)
    }
}

#[crate::view(component = Text, name = "Text", priority = 80, invalidation = Layout, attach = crate::text::path::attach_text_path::callback())]
#[allow(
    clippy::too_many_arguments,
    reason = "Each parameter declares an independent paint dependency."
)]
fn text_render(
    rows: query!(Text, optional crate::text::TextLayoutHandle, optional crate::text::TextPath),
    entity: entity!(),
    theme: res!(optional crate::ui::Theme),
    outputs: res!(optional crate::ui::Outputs<crate::text::TextGeometry>),
    manager: res!(optional crate::render::font::FontManager),
    paths: res!(optional crate::render::path::PathStore),
    paint: paint!(),
) {
    let Some((text, handle, path)) = rows.get(entity) else {
        return;
    };
    let theme =
        theme.expect("App::new must insert Theme; missing means a test fixture skipped App");
    let color = paint.style().text_color.resolve_in(theme, paint.state());
    let Some(outputs) = outputs else {
        return;
    };
    let storage = outputs.storage();
    let face_limit = storage.layouts.limits().fallback_faces;
    let Ok(Some(fonts)) = crate::render::font::ResolvedFontStack::resolve(
        manager,
        &paint.style().font_stack,
        paint.style().font_size,
        face_limit,
    ) else {
        return;
    };
    let Some(handle) = handle.copied() else {
        return;
    };
    let Some(layout) = storage.layouts.get(handle) else {
        return;
    };
    if let Some(path) = path.copied() {
        let Some(paths) = paths else {
            return;
        };
        let Ok(revision) = paths.revision(path.path()) else {
            paint.fail(crate::render::RenderError::InvalidGeometry);
            return;
        };
        let key = PlacementKey::new(
            entity,
            handle,
            path,
            revision,
            crate::text::baseline::DEFAULT_TOLERANCE,
        );
        let rect = paint.rect();
        let transform = paint.transform();
        let clip = paint.clip();
        let result = storage.paths.glyph_frames(key, |frames| {
            draw_posed_text_layout(
                paint,
                &layout,
                frames,
                |font_id| fonts.font(font_id),
                TextPaint::new(
                    Point {
                        x: rect.x,
                        y: rect.y,
                    },
                    transform,
                    &clip,
                    color,
                ),
            )
        });
        if result.is_none() {
            paint.fail(crate::render::RenderError::InvalidGeometry);
        }
        return;
    }
    let content_rect = linear_text_content_rect(paint.style(), paint.rect());
    let offset_y = vertical_offset(
        text.paragraph().vertical_align,
        content_rect.h,
        crate::types::fixed::from_textflow(layout.measure().height),
    );
    let transform = paint.transform();
    let clip = paint.clip();
    draw_text_layout_with_identity(
        paint,
        &layout,
        |font_id| fonts.font(font_id),
        TextPaint::new(
            Point {
                x: content_rect.x,
                y: content_rect.y + offset_y,
            },
            transform,
            &clip,
            color,
        ),
        text.text_capacity().map(|_| entity),
    );
}

pub(crate) fn linear_text_content_rect(style: &crate::ui::Style, rect: Rect) -> Rect {
    let padding = style.layout.padding;
    let left = padding.left.resolve(rect.w).unwrap_or(Fixed::ZERO);
    let right = padding.right.resolve(rect.w).unwrap_or(Fixed::ZERO);
    let top = padding.top.resolve(rect.h).unwrap_or(Fixed::ZERO);
    let bottom = padding.bottom.resolve(rect.h).unwrap_or(Fixed::ZERO);
    Rect {
        x: rect.x + left,
        y: rect.y + top,
        w: (rect.w - left - right).max(Fixed::ZERO),
        h: (rect.h - top - bottom).max(Fixed::ZERO),
    }
}

pub(crate) fn padded_text_intrinsic_size(
    style: &crate::ui::Style,
    width: Fixed,
    height: Fixed,
) -> (Fixed, Fixed) {
    let padding = style.layout.padding;
    (
        padded_intrinsic_axis(width, padding.left, padding.right),
        padded_intrinsic_axis(height, padding.top, padding.bottom),
    )
}

fn padded_intrinsic_axis(
    content: Fixed,
    before: crate::types::Dimension,
    after: crate::types::Dimension,
) -> Fixed {
    use crate::types::Dimension;

    let mut fixed = Fixed::ZERO;
    let mut percent = Fixed::ZERO;
    for value in [before, after] {
        match value {
            Dimension::Px(value) => fixed += value,
            Dimension::Percent(value) => percent += value,
            Dimension::Auto | Dimension::Content => {}
        }
    }
    let base = (content + fixed).max(Fixed::ZERO);
    let hundred = Fixed::from_int(100);
    let remaining = hundred - percent;
    if remaining > Fixed::ZERO {
        (base * hundred / remaining).max(Fixed::ZERO)
    } else {
        base
    }
}

fn draw_posed_text_layout<'font>(
    paint_ctx: &mut PaintCtx<'_>,
    layout: &crate::text::TextLayout<'_>,
    frames: &[textflow::placement::GlyphFrame],
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    text_paint: TextPaint<'_>,
) {
    for_each_posed_run(layout, frames, font_for, |font, glyphs| {
        if paint_ctx.error().is_some() {
            return;
        }
        paint_ctx.draw_clipped(
            &DrawCommand::PosedGlyphRun {
                pos: text_paint.origin,
                transform: text_paint.transform,
                glyphs,
                font,
                color: text_paint.color,
                opa: 255,
            },
            *text_paint.clip,
        );
    });
}

fn for_each_posed_run<'font>(
    layout: &crate::text::TextLayout<'_>,
    frames: &[textflow::placement::GlyphFrame],
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    mut visit: impl FnMut(&'font crate::render::font::Font, crate::render::PosedGlyphs<'_>),
) {
    for line in layout.lines() {
        let Some(runs) = layout.runs_for(*line) else {
            continue;
        };
        for run in runs {
            let Some(font) = font_for(run.font_id()) else {
                continue;
            };
            let range = run.glyphs();
            let range = range.start as usize..range.end as usize;
            let Some(glyphs) = layout.glyphs().get(range.clone()) else {
                continue;
            };
            let Some(frames) = frames.get(range) else {
                continue;
            };
            let Some(glyphs) = crate::render::PosedGlyphs::new(glyphs, frames) else {
                continue;
            };
            visit(font, glyphs);
        }
    }
}

pub(crate) fn path_text_ink_bounds(
    world: &World,
    entity: Entity,
    rect: Rect,
    transform: Transform,
    output_scale: Fixed,
) -> Option<Rect> {
    let text_path = world.get::<crate::text::TextPath>(entity).copied()?;
    let paths = world.resource::<crate::render::path::PathStore>()?;
    let style = world.get::<crate::ui::Style>(entity)?;
    let outputs = world.resource::<crate::ui::Outputs<crate::text::TextGeometry>>()?;
    let storage = outputs.storage();
    let face_limit = storage.layouts.limits().fallback_faces;
    let fonts = crate::render::font::ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .ok()??;
    let handle = world
        .get::<crate::text::TextLayoutHandle>(entity)
        .copied()?;
    let layout = storage.layouts.get(handle)?;
    let key = PlacementKey::new(
        entity,
        handle,
        text_path,
        paths.revision(text_path.path()).ok()?,
        crate::text::baseline::DEFAULT_TOLERANCE,
    );
    storage
        .paths
        .glyph_frames(key, |frames| {
            let mut bounds: Option<Rect> = None;
            for_each_posed_run(
                &layout,
                frames,
                |font_id| fonts.font(font_id),
                |font, glyphs| {
                    if let Some(run_bounds) = glyphs.ink_bounds(
                        font,
                        Point {
                            x: rect.x,
                            y: rect.y,
                        },
                        transform,
                        output_scale,
                    ) {
                        bounds = Some(match bounds {
                            Some(current) => current.union(&run_bounds),
                            None => run_bounds,
                        });
                    }
                },
            );
            bounds
        })
        .flatten()
}

/// The nearest visual caret returned by a path-text hit query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathCaretHit {
    index: usize,
    text_offset: u32,
    bidi_level: u8,
}

impl PathCaretHit {
    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn text_offset(self) -> u32 {
        self.text_offset
    }

    pub const fn bidi_level(self) -> u8 {
        self.bidi_level
    }
}

/// One source-local caret segment on a path-text widget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathCaretGeometry {
    segment: [Point; 2],
    index: usize,
    text_offset: u32,
    bidi_level: u8,
}

impl Default for PathCaretGeometry {
    fn default() -> Self {
        Self {
            segment: [Point::ZERO; 2],
            index: 0,
            text_offset: 0,
            bidi_level: 0,
        }
    }
}

impl PathCaretGeometry {
    pub const fn segment(self) -> [Point; 2] {
        self.segment
    }

    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn text_offset(self) -> u32 {
        self.text_offset
    }

    pub const fn bidi_level(self) -> u8 {
        self.bidi_level
    }
}

/// One source-local selection segment within a line and visual bidi run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathSelectionRibbon {
    quad: [Point; 4],
    text_start: u32,
    text_end: u32,
    bidi_level: u8,
}

impl Default for PathSelectionRibbon {
    fn default() -> Self {
        Self {
            quad: [Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            }; 4],
            text_start: 0,
            text_end: 0,
            bidi_level: 0,
        }
    }
}

impl PathSelectionRibbon {
    pub const fn quad(self) -> [Point; 4] {
        self.quad
    }

    pub const fn text_range(self) -> Range<u32> {
        self.text_start..self.text_end
    }

    pub const fn bidi_level(self) -> u8 {
        self.bidi_level
    }
}

/// Failure to resolve or project path-text interaction geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTextGeometryError {
    /// Required retained layout or path data is unavailable.
    Unavailable,
    /// The geometry is singular or crosses the projective near plane.
    InvalidProjection,
    /// Caller-provided geometry storage is too small.
    InsufficientCapacity { required: usize, provided: usize },
}

impl crate::text::TextGeometryRead<'_> {
    /// Finds the nearest caret within `max_distance` of a projected point.
    pub fn hit_test(
        &self,
        point: Point,
        max_distance: Fixed,
    ) -> Result<Option<PathCaretHit>, PathTextGeometryError> {
        let limit = square_wide(Fixed64::from_fixed(max_distance.max(Fixed::ZERO)));
        self.with_path_carets(|layout, frames, metrics| {
            let mut nearest = None;
            for (index, (caret, frame)) in layout.carets().iter().zip(frames).enumerate() {
                let (start, end) = caret_segment(Rect::ZERO, self.transform(), *frame, metrics)
                    .ok_or(PathTextGeometryError::InvalidProjection)?;
                let distance = point_segment_distance_squared(point, start, end);
                if distance > limit
                    || nearest
                        .as_ref()
                        .is_some_and(|(_, best_distance)| distance >= *best_distance)
                {
                    continue;
                }
                nearest = Some((
                    PathCaretHit {
                        index,
                        text_offset: caret.text_offset,
                        bidi_level: caret.bidi_level,
                    },
                    distance,
                ));
            }
            Ok(nearest.map(|(hit, _)| hit))
        })
        .ok_or(PathTextGeometryError::Unavailable)?
    }

    /// Writes every source-local path caret segment into caller-provided storage.
    pub fn carets_into<'output>(
        &self,
        output: &'output mut [PathCaretGeometry],
    ) -> Result<&'output [PathCaretGeometry], PathTextGeometryError> {
        self.with_path_carets(|layout, frames, metrics| {
            let required = layout.carets().len();
            if output.len() < required {
                return Err(PathTextGeometryError::InsufficientCapacity {
                    required,
                    provided: output.len(),
                });
            }
            if frames.iter().any(|frame| {
                caret_segment(Rect::ZERO, Transform3D::IDENTITY, *frame, metrics).is_none()
            }) {
                return Err(PathTextGeometryError::InvalidProjection);
            }
            for (index, (slot, (caret, frame))) in output
                .iter_mut()
                .zip(layout.carets().iter().zip(frames))
                .enumerate()
            {
                *slot = PathCaretGeometry {
                    segment: caret_segment(Rect::ZERO, Transform3D::IDENTITY, *frame, metrics)
                        .expect("caret projection was preflighted")
                        .into(),
                    index,
                    text_offset: caret.text_offset,
                    bidi_level: caret.bidi_level,
                };
            }
            Ok(&output[..required])
        })
        .ok_or(PathTextGeometryError::Unavailable)?
    }

    /// Writes source-local path selection ribbons into caller-provided storage.
    pub fn selection_into<'output>(
        &self,
        selection: Range<u32>,
        output: &'output mut [PathSelectionRibbon],
    ) -> Result<&'output [PathSelectionRibbon], PathTextGeometryError> {
        self.with_path_carets(|layout, frames, metrics| {
            let required = selected_caret_pairs(layout, frames, selection.clone()).count();
            if output.len() < required {
                return Err(PathTextGeometryError::InsufficientCapacity {
                    required,
                    provided: output.len(),
                });
            }
            if selected_caret_pairs(layout, frames, selection.clone()).any(|pair| {
                selection_ribbon(Rect::ZERO, Transform3D::IDENTITY, metrics, pair).is_none()
            }) {
                return Err(PathTextGeometryError::InvalidProjection);
            }
            for (slot, pair) in output
                .iter_mut()
                .zip(selected_caret_pairs(layout, frames, selection))
            {
                *slot = selection_ribbon(Rect::ZERO, Transform3D::IDENTITY, metrics, pair)
                    .expect("selection projection was preflighted");
            }
            Ok(&output[..required])
        })
        .ok_or(PathTextGeometryError::Unavailable)?
    }
}

#[derive(Clone, Copy)]
struct SelectedCaretPair {
    start: textflow::shaping::CaretStop,
    start_frame: textflow::placement::CaretFrame,
    end: textflow::shaping::CaretStop,
    end_frame: textflow::placement::CaretFrame,
}

fn selected_caret_pairs<'a>(
    layout: &'a textflow::layout::ParagraphLayout<'a>,
    frames: &'a [textflow::placement::CaretFrame],
    selection: Range<u32>,
) -> impl Iterator<Item = SelectedCaretPair> + 'a {
    layout.lines().iter().flat_map(move |line| {
        let caret_range = line.carets();
        let carets = &layout.carets()[caret_range.start as usize..caret_range.end as usize];
        let frames = &frames[caret_range.start as usize..caret_range.end as usize];
        let run_range = line.runs();
        layout.runs()[run_range.start as usize..run_range.end as usize]
            .iter()
            .flat_map({
                let selection = selection.clone();
                move |run| {
                    carets.windows(2).zip(frames.windows(2)).filter_map({
                        let selection = selection.clone();
                        move |(carets, frames)| {
                            selected_pair(carets, run.text(), run.bidi_level(), &selection)
                                .then_some(SelectedCaretPair {
                                    start: carets[0],
                                    start_frame: frames[0],
                                    end: carets[1],
                                    end_frame: frames[1],
                                })
                        }
                    })
                }
            })
    })
}

fn selected_pair(
    carets: &[textflow::shaping::CaretStop],
    run: textflow::shaping::TextRange,
    bidi_level: u8,
    selection: &Range<u32>,
) -> bool {
    let start = carets[0].text_offset.min(carets[1].text_offset);
    let end = carets[0].text_offset.max(carets[1].text_offset);
    start < end
        && start >= run.start
        && end <= run.end
        && start < selection.end
        && end > selection.start
        && carets[0].bidi_level == bidi_level
        && carets[1].bidi_level == bidi_level
}

fn selection_ribbon(
    rect: Rect,
    transform: Transform3D,
    metrics: crate::render::font::FontMetrics,
    pair: SelectedCaretPair,
) -> Option<PathSelectionRibbon> {
    let (start_top, start_bottom) = caret_segment(rect, transform, pair.start_frame, metrics)?;
    let (end_top, end_bottom) = caret_segment(rect, transform, pair.end_frame, metrics)?;
    Some(PathSelectionRibbon {
        quad: [start_top, end_top, end_bottom, start_bottom],
        text_start: pair.start.text_offset.min(pair.end.text_offset),
        text_end: pair.start.text_offset.max(pair.end.text_offset),
        bidi_level: pair.start.bidi_level,
    })
}

fn caret_segment(
    rect: Rect,
    transform: Transform3D,
    frame: textflow::placement::CaretFrame,
    metrics: crate::render::font::FontMetrics,
) -> Option<(Point, Point)> {
    let origin = Point {
        x: rect.x + crate::types::fixed::from_textflow(frame.local_origin.x),
        y: rect.y + crate::types::fixed::from_textflow(frame.local_origin.y),
    };
    let tangent = Point {
        x: crate::types::fixed::from_textflow(frame.unit_tangent.x),
        y: crate::types::fixed::from_textflow(frame.unit_tangent.y),
    };
    let normal = Point {
        x: Fixed::ZERO - tangent.y,
        y: tangent.x,
    };
    Some((
        transform.apply_point(Point {
            x: origin.x - normal.x * metrics.ascender,
            y: origin.y - normal.y * metrics.ascender,
        })?,
        transform.apply_point(Point {
            x: origin.x + normal.x * (metrics.line_height - metrics.ascender),
            y: origin.y + normal.y * (metrics.line_height - metrics.ascender),
        })?,
    ))
}

fn square_wide(value: Fixed64) -> Fixed64 {
    value.mul_wide(value)
}

fn point_segment_distance_squared(point: Point, start: Point, end: Point) -> Fixed64 {
    let ax = Fixed64::from_fixed(start.x);
    let ay = Fixed64::from_fixed(start.y);
    let vx = Fixed64::from_fixed(end.x) - ax;
    let vy = Fixed64::from_fixed(end.y) - ay;
    let wx = Fixed64::from_fixed(point.x) - ax;
    let wy = Fixed64::from_fixed(point.y) - ay;
    let length_squared = square_wide(vx) + square_wide(vy);
    if length_squared.is_zero() {
        return square_wide(wx) + square_wide(wy);
    }
    let projection = wx.mul_wide(vx) + wy.mul_wide(vy);
    if !projection.is_positive() {
        return square_wide(wx) + square_wide(wy);
    }
    if projection >= length_squared {
        let dx = wx - vx;
        let dy = wy - vy;
        return square_wide(dx) + square_wide(dy);
    }
    let cross = wx.mul_wide(vy) - wy.mul_wide(vx);
    square_wide(cross).div_wide(length_squared)
}

pub(crate) fn vertical_offset(
    align: TextVerticalAlign,
    box_height: Fixed,
    text_height: Fixed,
) -> Fixed {
    let free_height = (box_height - text_height).max(Fixed::ZERO);
    match align {
        TextVerticalAlign::Start => Fixed::ZERO,
        TextVerticalAlign::Center => free_height / Fixed::from_int(2),
        TextVerticalAlign::End => free_height,
    }
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
    paint_ctx: &mut PaintCtx<'_>,
    layout: &crate::text::TextLayout<'_>,
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    text_paint: TextPaint<'_>,
) {
    draw_text_layout_with_identity(paint_ctx, layout, font_for, text_paint, None)
}

fn draw_text_layout_with_identity<'font>(
    paint_ctx: &mut PaintCtx<'_>,
    layout: &crate::text::TextLayout<'_>,
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    text_paint: TextPaint<'_>,
    text_entity: Option<Entity>,
) {
    for line in layout.lines() {
        let Some(runs) = layout.runs_for(*line) else {
            continue;
        };
        for (offset, run) in runs.iter().enumerate() {
            if paint_ctx.error().is_some() {
                return;
            }
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
            let command = DrawCommand::GlyphRun {
                pos: Point {
                    x: text_paint.origin.x + crate::types::fixed::from_textflow(glyph_origin.x),
                    y: text_paint.origin.y + crate::types::fixed::from_textflow(glyph_origin.y)
                        - metrics.ascender,
                },
                transform: text_paint.transform,
                glyphs: layout.glyphs_for(*run).unwrap_or_default(),
                font,
                color: text_paint.color,
                opa: 255,
            };
            if let Some(entity) = text_entity {
                paint_ctx.draw_text_run(
                    &command,
                    *text_paint.clip,
                    crate::render::TextRunIdentity {
                        entity,
                        run_index: line.runs().start as usize + offset,
                    },
                );
            } else {
                paint_ctx.draw_clipped(&command, *text_paint.clip);
            }
        }
    }
}

/// Borrowed positioned glyphs for rendering without runtime shaping or layout storage.
#[crate::component]
#[derive(Clone, Copy, Debug)]
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

#[crate::view(
    component = StaticGlyphRun,
    name = "StaticGlyphRun",
    priority = 80,
    invalidation = Layout,
)]
fn static_glyph_run_render(
    run: component!(),
    fonts: res!(optional crate::render::font::FontManager),
    theme: res!(optional crate::ui::Theme),
    paint: paint!(),
) {
    let Some(first) = run.glyphs.first() else {
        return;
    };
    let Some(manager) = fonts else {
        return;
    };
    let font = manager.resolve(paint.style().font_stack.primary().cache_key());
    let mut font = font.as_ref().clone();
    font.size = paint.style().font_size.unwrap_or(font.size).max(1);
    let metrics = font.metrics(font.size);
    let theme =
        theme.expect("App::new must insert Theme; missing means a test fixture skipped App");
    let rect = paint.rect();
    paint.draw(&DrawCommand::GlyphRun {
        pos: Point {
            x: rect.x + crate::types::fixed::from_textflow(first.origin.x),
            y: rect.y + crate::types::fixed::from_textflow(first.origin.y) - metrics.ascender,
        },
        transform: paint.transform(),
        glyphs: run.glyphs,
        font: &font,
        color: paint.style().text_color.resolve_in(theme, paint.state()),
        opa: 255,
    });
}

pub use text_render::view;

pub use static_glyph_run_render::view as static_glyph_run_view;

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::render::font::{GlyphId, default_font_manager};
    use crate::render::renderer::Renderer;
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::theme::{Theme, WidgetState};
    use textflow::shaping::{FlowPoint, PositionedGlyph};

    #[derive(Default)]
    struct RecordingRenderer {
        glyph_runs: Vec<(Point, usize)>,
    }

    #[crate::component]
    struct LayoutFollower {
        source: Entity,
    }

    #[crate::view(component = LayoutFollower)]
    fn follow_layout(
        follower: component!(),
        handles: query!(crate::text::TextLayoutHandle),
        calls: res!(core::cell::Cell<usize>),
    ) {
        if handles.get(follower.source).is_some() {
            calls.set(calls.get() + 1);
        }
    }

    impl Renderer for RecordingRenderer {
        fn route(
            &self,
            _: &crate::render::DrawRequest<'_, '_>,
        ) -> Result<crate::render::RenderRoute, crate::render::RenderError> {
            Ok(crate::render::RenderRoute::Native)
        }

        fn submit(
            &mut self,
            request: &crate::render::DrawRequest<'_, '_>,
        ) -> Result<(), crate::render::RenderError> {
            self.route(request)?;
            if let DrawCommand::GlyphRun { pos, glyphs, .. } = request.command {
                self.glyph_runs.push((*pos, glyphs.len()));
            }
            Ok(())
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
        let mut renderer = RecordingRenderer::default();
        {
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            static_glyph_run_view().render().unwrap()(&world, entity, &mut paint);
        }

        assert_eq!(renderer.glyph_runs, vec![(Point::new(3, 5), 2)]);
        assert_eq!(
            StaticGlyphRun::new(&GLYPHS).measure(Fixed::from_int(8)),
            (Fixed::from_int(16), Fixed::from_int(8))
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn static_run_typed_roles_refetch_resources_and_borrow_glyphs_without_allocating() {
        use crate::render::font::{Font, FontBackend, FontToken};
        use crate::render::{DrawRequest, RenderError};
        use crate::test_support::tracked_allocations;
        use core::any::TypeId;

        static GLYPHS: [PositionedGlyph; 1] = [PositionedGlyph::new(
            GlyphId::new(65),
            FlowPoint { x: 0, y: 7 << 8 },
        )];
        struct Trace {
            expected_color: crate::types::Color,
            family: &'static str,
            size: u16,
            fail: bool,
            calls: usize,
        }
        impl Renderer for Trace {
            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                assert_eq!(request.clip, Rect::new(4, 6, 18, 8));
                let DrawCommand::GlyphRun {
                    pos,
                    transform,
                    glyphs,
                    font,
                    color,
                    opa,
                } = request.command
                else {
                    panic!("expected a borrowed static glyph run");
                };
                assert_eq!(*pos, Point::new(3, 5));
                assert_eq!(
                    *transform,
                    Transform::translate(Fixed::from_int(2), Fixed::from_int(3))
                );
                assert_eq!(glyphs.as_ptr(), GLYPHS.as_ptr());
                assert_eq!(glyphs.len(), GLYPHS.len());
                assert_eq!((font.family, font.size), (self.family, self.size));
                assert_eq!(*color, self.expected_color);
                assert_eq!(*opa, 255);
                self.calls += 1;
                if self.fail {
                    Err(RenderError::BackendFailure)
                } else {
                    Ok(())
                }
            }
            fn flush(&mut self) {}
        }

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, StaticGlyphRun::new(&GLYPHS));
        let mut render = None;
        for _ in 0..2 {
            let mut descriptor = None;
            assert_eq!(
                tracked_allocations(|| descriptor = Some(static_glyph_run_view())),
                0
            );
            let descriptor = descriptor.unwrap();
            assert_eq!(descriptor.name(), "StaticGlyphRun");
            assert_eq!(
                descriptor.component_filter(),
                Some(TypeId::of::<StaticGlyphRun>())
            );
            assert_eq!(descriptor.priority(), Some(80));
            assert_eq!(
                descriptor.invalidation(),
                crate::ui::ViewInvalidation::Layout
            );
            assert!(descriptor.auto_attach().is_none());
            assert!(descriptor.inputs().is_none());
            assert!(descriptor.observation().is_none());
            let mut installed = 0;
            let descriptor = descriptor.install(|_| installed += 1);
            assert_eq!(installed, 0);
            render = descriptor.render();
        }
        let render = render.unwrap();
        let mut style = Style {
            font_stack: FontToken::Heading.into(),
            ..Style::default()
        };
        let rect = Rect::new(3, 5, 20, 10);
        let clip = Rect::new(4, 6, 18, 8);
        for (family, size, theme, override_size) in [
            ("first-static-font", 16, Theme::dark(), None),
            ("replacement-static-font", 20, Theme::light(), Some(0)),
        ] {
            let fonts = default_font_manager();
            fonts.add_static(
                FontToken::Heading.cache_key(),
                Font {
                    family,
                    size,
                    backend: FontBackend::Bitmap8x8,
                },
            );
            // Prepare the existing font cache before checking the typed paint adapter.
            fonts.prefetch(FontToken::Heading.cache_key());
            world.insert_resource(fonts);
            style.font_size = override_size;
            let expected_color = style.text_color.resolve_in(&theme, WidgetState::Hovered);
            world.insert_resource(theme);
            for fail in [false, true] {
                for _ in 0..2 {
                    let mut trace = Trace {
                        expected_color,
                        family,
                        size: override_size.unwrap_or(size).max(1),
                        fail,
                        calls: 0,
                    };
                    {
                        let mut paint = PaintCtx::new(
                            &mut trace,
                            &style,
                            &rect,
                            &clip,
                            Transform::translate(Fixed::from_int(2), Fixed::from_int(3)),
                            None,
                            WidgetState::Hovered,
                        );
                        assert_eq!(
                            tracked_allocations(|| render(&world, entity, &mut paint)),
                            0
                        );
                        assert_eq!(paint.error(), fail.then_some(RenderError::BackendFailure));
                        assert!(!paint.background_handled());
                    }
                    assert_eq!(trace.calls, 1);
                }
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn static_run_optional_resources_preserve_early_returns_and_the_theme_requirement() {
        use crate::render::font::{FontManager, FontToken};
        use crate::test_support::tracked_allocations;

        static GLYPHS: [PositionedGlyph; 1] = [PositionedGlyph::new(
            GlyphId::new(65),
            FlowPoint { x: 0, y: 7 << 8 },
        )];
        let mut world = World::new();
        let entity = world.spawn_empty();
        let style = Style::default();
        let rect = Rect::new(0, 0, 20, 10);
        let render = static_glyph_run_view().render().unwrap();
        let mut renderer = RecordingRenderer::default();
        for stage in 0..3 {
            world.insert(
                entity,
                StaticGlyphRun::new(if stage == 2 { &GLYPHS } else { &[] }),
            );
            if stage == 1 {
                world.insert_resource(default_font_manager());
            } else {
                world.remove_resource::<FontManager>();
            }
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(
                tracked_allocations(|| render(&world, entity, &mut paint)),
                0
            );
            assert_eq!(paint.error(), None);
        }
        assert!(renderer.glyph_runs.is_empty());
        let fonts = default_font_manager();
        fonts.prefetch(FontToken::Default.cache_key());
        world.insert_resource(fonts);
        {
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render(&world, entity, &mut paint);
            }))
            .unwrap_err();
            let message = error
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| error.downcast_ref::<String>().map(String::as_str))
                .unwrap();
            assert_eq!(
                message,
                "App::new must insert Theme; missing means a test fixture skipped App"
            );
        }
        assert!(renderer.glyph_runs.is_empty());
    }

    #[test]
    fn typed_paint_preserves_missing_dependencies_and_theme_validation() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let descriptor = view();
        assert_eq!(descriptor.name(), "Text");
        assert_eq!(descriptor.priority(), Some(80));
        assert_eq!(
            descriptor.component_filter(),
            Some(core::any::TypeId::of::<Text>())
        );
        assert_eq!(
            descriptor.invalidation(),
            crate::ui::ViewInvalidation::Layout
        );
        assert!(descriptor.auto_attach().is_some());
        assert!(descriptor.inputs().is_none());
        let render = descriptor.render().unwrap();
        let style = Style::default();
        let rect = Rect::new(0, 0, 20, 10);
        let mut recorder = RecordingRenderer::default();
        {
            let mut paint = PaintCtx::new(
                &mut recorder,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    let _ = view();
                    render(&world, entity, &mut paint);
                }),
                0
            );
            assert_eq!(paint.error(), None);
        }
        world.insert(entity, Text::new("A"));
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut paint = PaintCtx::new(
                &mut recorder,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            render(&world, entity, &mut paint);
        }));
        assert!(
            rejected.is_err(),
            "Text requires Theme even before layout is available"
        );
        world.insert_resource(Theme::default());
        for stage in 0..3 {
            match stage {
                1 => world.insert_resource(
                    crate::ui::Outputs::<crate::text::TextGeometry>::new_text(
                        crate::text::TextLayoutLimits::EMBEDDED,
                    ),
                ),
                2 => {
                    let fonts = default_font_manager();
                    fonts.prefetch(crate::render::font::FontToken::Default.cache_key());
                    world.insert_resource(fonts);
                }
                _ => {}
            }
            let mut paint = PaintCtx::new(
                &mut recorder,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| render(&world, entity, &mut paint)),
                0
            );
            assert_eq!(paint.error(), None);
        }
        assert!(recorder.glyph_runs.is_empty());
    }

    #[test]
    fn unchanged_layout_handles_do_not_publish_query_changes() {
        use crate::surface::Surface;

        let mut app = crate::app::App::headless(32, 16);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            crate::ui! {
                Text ("A", id: "handle_consumer", width: 20, height: 10)
            };
        });
        let label = app.world.find_by_id("handle_consumer").unwrap();
        app.render_dirty().unwrap();
        let handle = *app
            .world
            .get::<crate::text::TextLayoutHandle>(label)
            .unwrap();
        app.world.insert(root, crate::ui::dirty::Dirty);
        crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
            .unwrap();
        assert_eq!(
            app.world.get::<crate::text::TextLayoutHandle>(label),
            Some(&handle)
        );
        assert!(!app.world.has::<crate::ui::dirty::Dirty>(label));
        app.world.get_mut::<Text>(label).unwrap().set_content("B");
        crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
            .unwrap();
        assert_ne!(
            app.world.get::<crate::text::TextLayoutHandle>(label),
            Some(&handle)
        );
        assert!(app.world.has::<crate::ui::dirty::Dirty>(label));
    }

    #[test]
    fn replacing_the_layout_cache_repaints_queries_when_handle_numbers_are_reused() {
        let mut app = crate::app::App::headless(32, 16);
        app.with_default_widgets();
        app.with_widget(follow_layout::view());
        app.add_resource(core::cell::Cell::new(0_usize));
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            crate::ui! {
                Text ("A", id: "reused_handle", width: 20, height: 10)
            };
        });
        let label = app.world.find_by_id("reused_handle").unwrap();
        app.compose(root, |cx| {
            crate::ui! {
                View (width: 8, height: 6) [
                    LayoutFollower { source: label },
                ]
            };
        });
        app.render_dirty().unwrap();
        let handle = *app
            .world
            .get::<crate::text::TextLayoutHandle>(label)
            .unwrap();
        let calls = app
            .world
            .resource::<core::cell::Cell<usize>>()
            .unwrap()
            .get();
        assert!(calls > 0);
        app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
            .unwrap();
        app.world.get_mut::<Text>(label).unwrap().set_content("B");
        app.render_dirty().unwrap();
        assert_eq!(
            app.world.get::<crate::text::TextLayoutHandle>(label),
            Some(&handle)
        );
        assert!(
            app.world
                .resource::<core::cell::Cell<usize>>()
                .unwrap()
                .get()
                > calls
        );
    }

    #[test]
    fn build_spawns_text_with_style() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let e = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Text::build("hi").style(crate::ui::Style::default()));
        let text = world.get::<Text>(e).unwrap();
        assert_eq!(text.resolve(world.resource()), "hi");
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_initializes_default_style() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let e = crate::ui::UiScope::new(&mut world, builder_parent).spawn(Text::build("hi"));
        assert!(world.has::<Text>(e));
        assert!(world.has::<crate::ui::Style>(e));
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
    fn bounded_text_formats_utf8_without_replacing_valid_content_on_error() {
        let mut text = Text::new_bounded(8).unwrap();
        assert!(!text.has_valid_content());
        assert_eq!(text.try_set_bounded::<8>(format_args!("é🙂")), Ok(true));
        assert!(text.has_valid_content());
        assert_eq!(text.try_set_bounded::<8>(format_args!("é🙂")), Ok(false));
        assert_eq!(text.resolve(None), "é🙂");
        assert_eq!(
            text.try_set_bounded::<8>(format_args!("é{}", "🙂🙂")),
            Err(TextContentError::Capacity {
                required: 10,
                capacity: 8,
            })
        );
        assert_eq!(text.resolve(None), "é🙂");
        assert_eq!(
            text.last_content_error(),
            Some(TextContentError::Capacity {
                required: 10,
                capacity: 8,
            })
        );
        assert_eq!(text.try_set_bounded::<8>(format_args!("ok")), Ok(true));
        assert_eq!(text.last_content_error(), None);
    }

    #[test]
    fn bounded_text_rejects_format_errors_and_capacity_mismatch() {
        struct Broken;
        impl core::fmt::Display for Broken {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("partial")?;
                Err(core::fmt::Error)
            }
        }

        let mut text = Text::new_bounded(8).unwrap();
        assert_eq!(
            text.try_set_bounded::<8>(format_args!("{Broken}")),
            Err(TextContentError::Format)
        );
        assert!(!text.has_valid_content());
        assert_eq!(text.resolve(None), "");
        assert_eq!(
            text.try_set_bounded::<4>(format_args!("ok")),
            Err(TextContentError::CapacityMismatch {
                declared: 8,
                provided: 4,
            })
        );
        assert_eq!(text.try_set_bounded::<8>(format_args!("ok")), Ok(true));
        assert!(text.has_valid_content());
    }

    #[test]
    fn bounded_text_preserves_reserved_storage_when_cloned_or_set() {
        let mut text = Text::new("ready").with_text_capacity(8).unwrap();
        assert!(text.has_valid_content());
        let TextContent::Plain(Cow::Owned(storage)) = text.content() else {
            panic!("bounded text must own its content storage");
        };
        assert!(storage.capacity() >= 8);

        let cloned = text.clone();
        let TextContent::Plain(Cow::Owned(storage)) = cloned.content() else {
            panic!("cloned bounded text must own its content storage");
        };
        assert!(storage.capacity() >= 8);

        assert_eq!(text.try_set_content("ready"), Ok(false));
        assert_eq!(
            text.try_set_content("too many bytes"),
            Err(TextContentError::Capacity {
                required: 14,
                capacity: 8,
            })
        );
        assert_eq!(text.resolve(None), "ready");
        assert_eq!(
            text.try_set_content(Localized::new("welcome")),
            Err(TextContentError::Localized)
        );
        assert_eq!(text.resolve(None), "ready");
        assert_eq!(text.try_set_content("changed"), Ok(true));
        assert_eq!(text.last_content_error(), None);
        let TextContent::Plain(Cow::Owned(storage)) = text.content() else {
            panic!("bounded text must own its content storage");
        };
        assert!(storage.capacity() >= 8);
    }

    #[test]
    fn localized_text_cannot_declare_unresolved_content_capacity() {
        assert_eq!(
            Text::new(Localized::new("welcome"))
                .with_text_capacity(8)
                .unwrap_err(),
            TextContentError::Localized
        );
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
        assert_eq!(paragraph.vertical_align, TextVerticalAlign::Start);
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
    fn label_is_single_line_and_centered_on_both_axes() {
        let text = Text::label("Apply");
        let paragraph = text.paragraph();

        assert_eq!(paragraph.wrap, TextWrap::NoWrap);
        assert_eq!(paragraph.align, TextAlign::Center);
        assert_eq!(paragraph.vertical_align, TextVerticalAlign::Center);
        assert_eq!(paragraph.overflow, TextOverflow::Clip);
    }

    #[test]
    fn vertical_alignment_uses_remaining_box_height() {
        let box_height = Fixed::from_int(30);
        let text_height = Fixed::from_int(12);
        assert_eq!(
            vertical_offset(TextVerticalAlign::Start, box_height, text_height),
            Fixed::ZERO
        );
        assert_eq!(
            vertical_offset(TextVerticalAlign::Center, box_height, text_height),
            Fixed::from_int(9)
        );
        assert_eq!(
            vertical_offset(TextVerticalAlign::End, box_height, text_height),
            Fixed::from_int(18)
        );
        assert_eq!(
            vertical_offset(TextVerticalAlign::Center, text_height, box_height),
            Fixed::ZERO
        );
    }

    #[test]
    fn caret_hit_distance_clamps_to_the_segment() {
        let start = Point::new(10, 10);
        let end = Point::new(10, 20);

        assert_eq!(
            point_segment_distance_squared(Point::new(13, 15), start, end),
            Fixed64::from_int(9)
        );
        assert_eq!(
            point_segment_distance_squared(Point::new(13, 24), start, end),
            Fixed64::from_int(25)
        );
    }

    #[test]
    fn caret_hit_distance_handles_a_collapsed_segment() {
        let caret = Point::new(4, 8);

        assert_eq!(
            point_segment_distance_squared(Point::new(7, 12), caret, caret),
            Fixed64::from_int(25)
        );
    }

    #[test]
    fn selection_pairs_stay_inside_one_visual_run() {
        let ltr = [
            textflow::shaping::CaretStop {
                text_offset: 0,
                position: FlowPoint { x: 0, y: 0 },
                bidi_level: 0,
            },
            textflow::shaping::CaretStop {
                text_offset: 2,
                position: FlowPoint { x: 2 << 8, y: 0 },
                bidi_level: 0,
            },
        ];
        assert!(selected_pair(
            &ltr,
            textflow::shaping::TextRange::new(0, 3),
            0,
            &(1..3)
        ));
        assert!(!selected_pair(
            &ltr,
            textflow::shaping::TextRange::new(1, 3),
            0,
            &(0..3)
        ));

        let rtl = [
            textflow::shaping::CaretStop {
                text_offset: 4,
                position: FlowPoint { x: 0, y: 0 },
                bidi_level: 1,
            },
            textflow::shaping::CaretStop {
                text_offset: 2,
                position: FlowPoint { x: 2 << 8, y: 0 },
                bidi_level: 1,
            },
        ];
        assert!(selected_pair(
            &rtl,
            textflow::shaping::TextRange::new(2, 4),
            1,
            &(2..4)
        ));
    }

    #[test]
    fn selection_ribbon_uses_oriented_caret_edges() {
        let pair = SelectedCaretPair {
            start: textflow::shaping::CaretStop {
                text_offset: 0,
                position: FlowPoint::default(),
                bidi_level: 0,
            },
            start_frame: textflow::placement::CaretFrame {
                local_origin: FlowPoint {
                    x: 10 << 8,
                    y: 20 << 8,
                },
                unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
            },
            end: textflow::shaping::CaretStop {
                text_offset: 1,
                position: FlowPoint::default(),
                bidi_level: 0,
            },
            end_frame: textflow::placement::CaretFrame {
                local_origin: FlowPoint {
                    x: 20 << 8,
                    y: 20 << 8,
                },
                unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
            },
        };
        let ribbon = selection_ribbon(
            Rect::new(0, 0, 100, 40),
            Transform3D::IDENTITY,
            crate::render::font::FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-2),
                line_height: Fixed::from_int(10),
            },
            pair,
        )
        .unwrap();

        assert_eq!(
            ribbon.quad(),
            [
                Point::new(10, 13),
                Point::new(20, 13),
                Point::new(20, 23),
                Point::new(10, 23),
            ]
        );
        assert_eq!(ribbon.text_range(), 0..1);
    }

    #[test]
    fn selection_ribbon_rejects_carets_behind_the_projective_plane() {
        let pair = SelectedCaretPair {
            start: textflow::shaping::CaretStop {
                text_offset: 0,
                position: FlowPoint::default(),
                bidi_level: 0,
            },
            start_frame: textflow::placement::CaretFrame {
                local_origin: FlowPoint {
                    x: 10 << 8,
                    y: 20 << 8,
                },
                unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
            },
            end: textflow::shaping::CaretStop {
                text_offset: 1,
                position: FlowPoint::default(),
                bidi_level: 0,
            },
            end_frame: textflow::placement::CaretFrame {
                local_origin: FlowPoint {
                    x: 20 << 8,
                    y: 20 << 8,
                },
                unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
            },
        };
        let mut behind = Transform3D::IDENTITY;
        behind.m22 = -Fixed64::ONE;

        assert!(
            selection_ribbon(
                Rect::new(0, 0, 100, 40),
                behind,
                crate::render::font::FontMetrics {
                    ascender: Fixed::from_int(7),
                    descender: Fixed::from_int(-2),
                    line_height: Fixed::from_int(10),
                },
                pair,
            )
            .is_none()
        );
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
