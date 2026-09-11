use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use crate::core::i18n::Localized;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Fixed64, Point, Rect, Transform};
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
            path: None,
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
    path: Option<crate::text::TextPath>,
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
        if let Some(path) = self.path {
            crate::text::path::set_text_path(world, entity, path);
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
    if let Some(path) = world.get::<crate::text::TextPath>(entity).copied() {
        let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
            return;
        };
        let Some(path_cache) = world.resource::<crate::text::baseline::PathBaselineResource>()
        else {
            return;
        };
        let _ = path_cache.with_glyph_frames(
            paths,
            path,
            crate::text::baseline::DEFAULT_TOLERANCE,
            handle,
            &layout,
            |frames| {
                draw_posed_text_layout(
                    renderer,
                    &layout,
                    frames,
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
            },
        );
        return;
    }
    let offset_y = vertical_offset(
        text.paragraph().vertical_align,
        rect.h,
        crate::types::fixed::from_textflow(layout.measure().height),
    );
    draw_text_layout(
        renderer,
        &layout,
        |font_id| fonts.font(font_id),
        TextPaint::new(
            Point {
                x: rect.x,
                y: rect.y + offset_y,
            },
            ctx.transform,
            ctx.clip,
            color,
        ),
    );
}

fn draw_posed_text_layout<'font>(
    renderer: &mut dyn Renderer,
    layout: &crate::text::TextLayout<'_>,
    frames: &[textflow::placement::GlyphFrame],
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    paint: TextPaint<'_>,
) {
    for_each_posed_run(layout, frames, font_for, |font, glyphs| {
        renderer.draw(
            &DrawCommand::PosedGlyphRun {
                pos: paint.origin,
                transform: paint.transform,
                glyphs,
                font,
                color: paint.color,
                opa: 255,
            },
            paint.clip,
        );
    });
}

fn for_each_posed_run<'font>(
    layout: &crate::text::TextLayout<'_>,
    frames: &[textflow::placement::GlyphFrame],
    font_for: impl Fn(crate::render::font::FontFaceId) -> Option<&'font crate::render::font::Font>,
    mut visit: impl FnMut(&'font crate::render::font::Font, crate::render::command::PosedGlyphs<'_>),
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
            let Some(glyphs) = crate::render::command::PosedGlyphs::new(glyphs, frames) else {
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
    let path_cache = world.resource::<crate::text::baseline::PathBaselineResource>()?;
    let style = world.get::<crate::ui::Style>(entity)?;
    let layouts = world.resource::<crate::text::layout::TextLayoutResource>()?;
    let face_limit = layouts.borrow().limits().fallback_faces;
    let fonts = crate::render::font::ResolvedFontStack::resolve(
        world,
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .ok()??;
    let handle = world
        .get::<crate::text::TextLayoutHandle>(entity)
        .copied()?;
    let layouts = layouts.borrow();
    let layout = layouts.get(handle)?;
    path_cache
        .with_glyph_frames(
            paths,
            text_path,
            crate::text::baseline::DEFAULT_TOLERANCE,
            handle,
            &layout,
            |frames| {
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
            },
        )
        .ok()
        .flatten()
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTextGeometryError {
    Unavailable,
    InsufficientCapacity { required: usize, provided: usize },
}

pub struct PathTextGeometry<'a> {
    world: &'a World,
    entity: Entity,
    rect: Rect,
    transform: Transform,
}

impl<'a> PathTextGeometry<'a> {
    pub fn for_widget(
        world: &'a World,
        entity: Entity,
        rect: Rect,
        transform: Transform,
    ) -> Option<Self> {
        world.get::<Text>(entity)?;
        world.get::<crate::text::TextPath>(entity)?;
        world.get::<crate::text::TextLayoutHandle>(entity)?;
        Some(Self {
            world,
            entity,
            rect,
            transform,
        })
    }

    pub fn hit_test(&self, point: Point, max_distance: Fixed) -> Option<PathCaretHit> {
        let limit = square_wide(Fixed64::from_fixed(max_distance.max(Fixed::ZERO)));
        self.with_carets(|layout, frames, metrics| {
            let mut nearest = None;
            for (index, (caret, frame)) in layout.carets().iter().zip(frames).enumerate() {
                let (start, end) = caret_segment(self.rect, self.transform, *frame, metrics);
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
            nearest.map(|(hit, _)| hit)
        })
        .flatten()
    }

    pub fn selection_into<'output>(
        &self,
        selection: Range<u32>,
        output: &'output mut [PathSelectionRibbon],
    ) -> Result<&'output [PathSelectionRibbon], PathTextGeometryError> {
        self.with_carets(|layout, frames, metrics| {
            let required = selected_caret_pairs(layout, frames, selection.clone()).count();
            if output.len() < required {
                return Err(PathTextGeometryError::InsufficientCapacity {
                    required,
                    provided: output.len(),
                });
            }
            for (slot, pair) in output
                .iter_mut()
                .zip(selected_caret_pairs(layout, frames, selection))
            {
                *slot = selection_ribbon(self.rect, self.transform, metrics, pair);
            }
            Ok(&output[..required])
        })
        .ok_or(PathTextGeometryError::Unavailable)?
    }

    fn with_carets<R>(
        &self,
        inspect: impl FnOnce(
            &textflow::layout::ParagraphLayout<'_>,
            &[textflow::placement::CaretFrame],
            crate::render::font::FontMetrics,
        ) -> R,
    ) -> Option<R> {
        let text_path = self
            .world
            .get::<crate::text::TextPath>(self.entity)
            .copied()?;
        let paths = self.world.resource::<crate::render::path::PathStore>()?;
        let path_cache = self
            .world
            .resource::<crate::text::baseline::PathBaselineResource>()?;
        let style = self.world.get::<crate::ui::Style>(self.entity)?;
        let layouts = self
            .world
            .resource::<crate::text::layout::TextLayoutResource>()?;
        let face_limit = layouts.borrow().limits().fallback_faces;
        let fonts = crate::render::font::ResolvedFontStack::resolve(
            self.world,
            &style.font_stack,
            style.font_size,
            face_limit,
        )
        .ok()??;
        let metrics = fonts.primary().metrics(fonts.primary().size);
        let handle = self
            .world
            .get::<crate::text::TextLayoutHandle>(self.entity)
            .copied()?;
        let layouts = layouts.borrow();
        let layout = layouts.get(handle)?;
        let paragraph = layout.paragraph().ok()?;
        path_cache
            .with_caret_frames(
                paths,
                text_path,
                crate::text::baseline::DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| inspect(&paragraph, frames, metrics),
            )
            .ok()
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
    transform: Transform,
    metrics: crate::render::font::FontMetrics,
    pair: SelectedCaretPair,
) -> PathSelectionRibbon {
    let (start_top, start_bottom) = caret_segment(rect, transform, pair.start_frame, metrics);
    let (end_top, end_bottom) = caret_segment(rect, transform, pair.end_frame, metrics);
    PathSelectionRibbon {
        quad: [start_top, end_top, end_bottom, start_bottom],
        text_start: pair.start.text_offset.min(pair.end.text_offset),
        text_end: pair.start.text_offset.max(pair.end.text_offset),
        bidi_level: pair.start.bidi_level,
    }
}

fn caret_segment(
    rect: Rect,
    transform: Transform,
    frame: textflow::placement::CaretFrame,
    metrics: crate::render::font::FontMetrics,
) -> (Point, Point) {
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
    (
        transform.apply_point(Point {
            x: origin.x - normal.x * metrics.ascender,
            y: origin.y - normal.y * metrics.ascender,
        }),
        transform.apply_point(Point {
            x: origin.x + normal.x * (metrics.line_height - metrics.ascender),
            y: origin.y + normal.y * (metrics.line_height - metrics.ascender),
        }),
    )
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

fn vertical_offset(align: TextVerticalAlign, box_height: Fixed, text_height: Fixed) -> Fixed {
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
            Transform::IDENTITY,
            crate::render::font::FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-2),
                line_height: Fixed::from_int(10),
            },
            pair,
        );

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
