use super::TextEditing;
use crate::ecs::{Entity, World};
use crate::text::{TextGeometry, TextLayout};
use crate::types::{Dimension, Fixed, Point, Rect};
use crate::ui::layout::Padding;
use crate::ui::outputs::Outputs;
use crate::ui::widgets::Placeholder;

#[crate::component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TextEditingLayoutHandle(pub crate::text::TextLayoutHandle);

/// Clear space kept inside an input whose padding is not written, so that
/// the text never touches the border.
const LEAST_INSET: i32 = 2;
/// The padding above and below a line that sizes an input with no height of
/// its own; the line itself stays centred in what the height leaves.
pub(crate) const AUTO_HEIGHT_PADDING: i32 = 6;

/// How the text sits in the box the padding leaves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LineAlign {
    /// One line, centred between top and bottom, as a field is.
    Middle,
    /// Lines from the top down, as a text area is. Nothing lays out more than one
    /// line yet; the geometry is ready for the field that will.
    #[cfg_attr(not(test), allow(dead_code))]
    Top,
}

/// Where the text of an input is laid out.
///
/// Each side takes the padding the author wrote, in pixels or as a share of
/// the box, the way the layout reads it. A side left unwritten has a default:
/// left and right keep clear of the corner, three fifths of its radius in, so a
/// rounded or pill shaped field does not run its text into the arc; top and
/// bottom keep two pixels from the border. Writing zero puts the text flush.
/// `content_height` is the height of the text, one line or several.
pub(crate) fn input_content_rect(
    rect: Rect,
    radius: Fixed,
    content_height: Fixed,
    padding: Padding,
    align: LineAlign,
) -> Rect {
    let least = Fixed::from_int(LEAST_INSET);
    let corner = (radius * 3 / 5)
        .round()
        .max(least)
        .min(rect.w / Fixed::from_int(2));
    let side = |value: Dimension, of: Fixed, default: Fixed| {
        value.resolve(of).unwrap_or(default).max(Fixed::ZERO)
    };
    let left = side(padding.left, rect.w, corner);
    let right = side(padding.right, rect.w, corner);
    let top = side(padding.top, rect.h, least);
    let bottom = side(padding.bottom, rect.h, least);
    let width = (rect.w - left - right).max(Fixed::ZERO);
    let room = (rect.h - top - bottom).max(Fixed::ZERO);
    let height = content_height.min(room);
    let y = match align {
        LineAlign::Middle => rect.y + top + ((room - height) / Fixed::from_int(2)).floor(),
        LineAlign::Top => rect.y + top,
    };
    Rect {
        x: rect.x + left,
        y,
        w: width,
        h: height,
    }
}

/// What an input is laid out in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    /// The box the text takes, as tall as the rows that show.
    pub content: Rect,
    pub line_height: Fixed,
    /// How many rows show at once; one is a field, more is a text area.
    pub rows: u8,
}

/// The rectangle an input lays its text out in, for a widget drawn in `rect`.
#[cfg(test)]
pub(crate) fn content_rect_of(world: &World, entity: Entity, rect: Rect) -> Option<Rect> {
    with_layout_request(world, entity, rect, |_, _, _, field| Ok(field.content))
        .ok()
        .flatten()
}

pub(crate) fn prepare_layout(
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Result<Option<crate::text::TextLayoutHandle>, crate::ui::render_system::TextLayoutFailure> {
    with_layout_request(world, entity, rect, |fonts, request, fingerprint, _| {
        let outputs = world.resource::<Outputs<TextGeometry>>().unwrap();
        fonts
            .with_typefaces(
                None,
                crate::ui::widgets::ParagraphStyle::default().shaping,
                |typefaces| {
                    outputs.storage_mut().layouts.layout_cached(
                        u64::from(entity.id) | (u64::from(entity.generation) << 32),
                        fingerprint,
                        request,
                        typefaces,
                    )
                },
            )
            .map_err(|error| {
                crate::ui::render_system::TextLayoutFailure::text(
                    entity,
                    crate::ui::render_system::TextLayoutStage::Layout,
                    error,
                )
            })
    })
}

fn with_layout_request<R>(
    world: &World,
    entity: Entity,
    rect: Rect,
    use_request: impl FnOnce(
        &crate::render::font::ResolvedFontStack,
        crate::text::layout::TextLayoutRequest<'_>,
        u64,
        Field,
    ) -> Result<R, crate::ui::render_system::TextLayoutFailure>,
) -> Result<Option<R>, crate::ui::render_system::TextLayoutFailure> {
    use crate::ui::render_system::{TextLayoutFailure, TextLayoutStage};

    let Some(input) = world
        .get::<TextEditing>(entity)
        .filter(|input| input.usable(world))
    else {
        return Ok(None);
    };
    let Some(style) = world.get::<crate::ui::Style>(entity) else {
        return Ok(None);
    };
    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(None);
    };
    let face_limit = outputs.storage().layouts.limits().fallback_faces;
    let Some(fonts) = crate::render::font::ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(entity, TextLayoutStage::Layout, error.required, error.limit)
    })?
    else {
        return Ok(None);
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let rows = world
        .get::<crate::ui::widgets::TextInput>(entity)
        .map_or(1, |input| input.lines.max(1));
    let field = Field {
        content: input_content_rect(
            rect,
            style.border_radius,
            metrics.line_height * i32::from(rows),
            style.layout.padding,
            if rows > 1 {
                LineAlign::Top
            } else {
                LineAlign::Middle
            },
        ),
        line_height: metrics.line_height,
        rows,
    };
    input.read_display(|text| {
        let content = if text.is_empty() {
            world
                .get::<Placeholder>(entity)
                .map_or("", |placeholder| placeholder.0)
        } else {
            text
        };
        // A field keeps to one line and scrolls sideways; a text area wraps.
        let paragraph = if rows > 1 {
            crate::ui::widgets::ParagraphStyle::default()
        } else {
            crate::ui::widgets::ParagraphStyle {
                wrap: crate::ui::widgets::TextWrap::NoWrap,
                max_lines: Some(1),
                ..crate::ui::widgets::ParagraphStyle::default()
            }
        };
        let request = paragraph.layout_request(content, metrics, Some(field.content.w));
        let font_fingerprint = fonts.layout_fingerprint(None, paragraph.shaping);
        use_request(&fonts, request, font_fingerprint, field).map(Some)
    })
}

/// Where the caret is, in the layout: how far along its line and which line.
fn caret_stop(layout: &TextLayout<'_>, cursor: usize) -> Option<(Fixed, usize)> {
    (0..layout.lines().len()).find_map(|index| {
        stops(layout, index)
            .iter()
            .find(|stop| stop.text_offset as usize == cursor)
            .map(|stop| (crate::types::fixed::from_textflow(stop.position.x), index))
    })
}

/// The places on a line the caret can stand.
fn stops<'a>(layout: &'a TextLayout<'_>, line: usize) -> &'a [textflow::shaping::CaretStop] {
    layout
        .lines()
        .get(line)
        .and_then(|line| {
            let range = line.carets();
            layout
                .carets()
                .get(range.start as usize..range.end as usize)
        })
        .unwrap_or(&[])
}

/// What of the layout shows in the box: where the caret is, and how far the
/// text has scrolled so that it stays in view. A field scrolls sideways to its
/// caret; a text area scrolls up and down to the line of it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Viewport {
    pub caret: Option<Fixed>,
    pub line: usize,
    pub scroll_x: Fixed,
    pub scroll_y: Fixed,
}

pub(crate) fn viewport(layout: &TextLayout<'_>, editing: &TextEditing, field: &Field) -> Viewport {
    let (caret, line) = match editing.display_cursor() {
        None => (None, 0),
        Some(cursor) => match caret_stop(layout, cursor) {
            Some((x, line)) => (Some(x), line),
            None => (Some(Fixed::ZERO), 0),
        },
    };
    if field.rows > 1 {
        let hidden = (line + 1).saturating_sub(usize::from(field.rows));
        return Viewport {
            caret,
            line,
            scroll_x: Fixed::ZERO,
            scroll_y: field.line_height * hidden as i32,
        };
    }
    let visible_width = (field.content.w - Fixed::ONE).max(Fixed::ZERO);
    Viewport {
        caret,
        line,
        scroll_x: (caret.unwrap_or(Fixed::ZERO) - visible_width).max(Fixed::ZERO),
        scroll_y: Fixed::ZERO,
    }
}

pub(super) fn hit_caret(world: &World, entity: Entity, point: Point) -> Option<usize> {
    let editing = world
        .get::<TextEditing>(entity)
        .filter(|editor| editor.usable(world))?;
    let (rect, affine, projective) = crate::ui::geometry::widget_render_geometry(world, entity)?;
    let point = if projective.is_identity() {
        affine.inverse()?.apply_point(point)
    } else {
        projective.inverse()?.apply_point(point)?
    };
    let handle = world.get::<TextEditingLayoutHandle>(entity)?.0;
    with_layout_request(world, entity, rect, |_, request, fingerprint, field| {
        let outputs = world.resource::<Outputs<TextGeometry>>().unwrap();
        let storage = outputs.storage();
        let Some(layout) = storage.layouts.get_matching(
            handle,
            u64::from(entity.id) | (u64::from(entity.generation) << 32),
            fingerprint,
            request,
        ) else {
            return Ok(None);
        };
        if editing.read_display(str::is_empty) {
            return Ok(Some(0));
        }
        let view = viewport(&layout, editing, &field);
        let x = point.x - field.content.x + view.scroll_x;
        let y = point.y - field.content.y + view.scroll_y;
        // The line under the point, then the stop of it nearest along.
        let last = layout.lines().len().saturating_sub(1);
        let line = if field.line_height > Fixed::ZERO {
            (y / field.line_height).floor().to_int().max(0) as usize
        } else {
            0
        }
        .min(last);
        Ok(nearest(stops(&layout, line), x)
            .map(|stop| editing.document_offset(stop.text_offset as usize)))
    })
    .ok()
    .flatten()
    .flatten()
}

/// The stop on a line closest to `x`.
fn nearest(
    stops: &[textflow::shaping::CaretStop],
    x: Fixed,
) -> Option<&textflow::shaping::CaretStop> {
    stops
        .iter()
        .min_by_key(|stop| (crate::types::fixed::from_textflow(stop.position.x) - x).abs())
}

pub(crate) fn caret_rect(world: &World, entity: Entity) -> Option<Rect> {
    let editing = world
        .get::<TextEditing>(entity)
        .filter(|editor| editor.usable(world))?;
    let (rect, affine, projective) = crate::ui::geometry::widget_render_geometry(world, entity)?;
    let handle = world.get::<TextEditingLayoutHandle>(entity)?.0;
    with_layout_request(world, entity, rect, |_, request, fingerprint, field| {
        let outputs = world.resource::<Outputs<TextGeometry>>().unwrap();
        let storage = outputs.storage();
        let Some(layout) = storage.layouts.get_matching(
            handle,
            u64::from(entity.id) | (u64::from(entity.generation) << 32),
            fingerprint,
            request,
        ) else {
            return Ok(None);
        };
        let view = viewport(&layout, editing, &field);
        let Some(caret) = view.caret else {
            return Ok(None);
        };
        let caret = Rect {
            x: field.content.x + caret - view.scroll_x,
            y: field.content.y + field.line_height * view.line as i32 - view.scroll_y,
            w: Fixed::ONE,
            h: field.line_height.min(field.content.h),
        };
        Ok(if projective.is_identity() {
            Some(affine.apply_rect_bbox(caret))
        } else {
            projective
                .apply_rect(caret)
                .map(|points| Rect::bounding_quad(&points))
        })
    })
    .ok()
    .flatten()
    .flatten()
}

/// A move of the caret that follows the lines of the layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Motion {
    Up,
    Down,
    LineStart,
    LineEnd,
}

/// Where the caret goes for `motion`, as an offset in the document. Up and
/// down keep to the column they are in, as far as the line they go to has one.
pub(crate) fn visual_target(world: &World, entity: Entity, motion: Motion) -> Option<usize> {
    let editing = world
        .get::<TextEditing>(entity)
        .filter(|editor| editor.usable(world))?;
    let (rect, _, _) = crate::ui::geometry::widget_render_geometry(world, entity)?;
    let handle = world.get::<TextEditingLayoutHandle>(entity)?.0;
    with_layout_request(world, entity, rect, |_, request, fingerprint, _| {
        let outputs = world.resource::<Outputs<TextGeometry>>().unwrap();
        let storage = outputs.storage();
        let Some(layout) = storage.layouts.get_matching(
            handle,
            u64::from(entity.id) | (u64::from(entity.generation) << 32),
            fingerprint,
            request,
        ) else {
            return Ok(None);
        };
        let cursor = editing.display_cursor().unwrap_or(0);
        let (x, line) = caret_stop(&layout, cursor).unwrap_or((Fixed::ZERO, 0));
        let last = layout.lines().len().saturating_sub(1);
        let stop = match motion {
            Motion::LineStart => stops(&layout, line).first(),
            Motion::LineEnd => stops(&layout, line).last(),
            Motion::Up if line == 0 => stops(&layout, 0).first(),
            Motion::Up => nearest(stops(&layout, line - 1), x),
            Motion::Down if line >= last => stops(&layout, last).last(),
            Motion::Down => nearest(stops(&layout, line + 1), x),
        };
        Ok(stop.map(|stop| editing.document_offset(stop.text_offset as usize)))
    })
    .ok()
    .flatten()
    .flatten()
}
