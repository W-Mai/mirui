use super::TextEditing;
use crate::ecs::{Entity, World};
use crate::text::{TextGeometry, TextLayout};
use crate::types::{Fixed, Point, Rect};
use crate::ui::outputs::Outputs;
use crate::ui::widgets::Placeholder;

#[crate::component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TextEditingLayoutHandle(pub crate::text::TextLayoutHandle);

pub(crate) fn input_content_rect(rect: Rect) -> Rect {
    let inset = Fixed::from_int(2);
    Rect {
        x: rect.x + inset,
        y: rect.y + inset,
        w: (rect.w - inset * Fixed::from_int(2)).max(Fixed::ZERO),
        h: (rect.h - inset * Fixed::from_int(2)).max(Fixed::ZERO),
    }
}

pub(crate) fn prepare_layout(
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Result<Option<crate::text::TextLayoutHandle>, crate::ui::render_system::TextLayoutFailure> {
    with_layout_request(world, entity, rect, |fonts, request, fingerprint| {
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
    input.read(|text| {
        let content = if text.is_empty() {
            world
                .get::<Placeholder>(entity)
                .map_or("", |placeholder| placeholder.0)
        } else {
            text
        };
        let paragraph = crate::ui::widgets::ParagraphStyle {
            wrap: crate::ui::widgets::TextWrap::NoWrap,
            max_lines: Some(1),
            ..crate::ui::widgets::ParagraphStyle::default()
        };
        let request = paragraph.layout_request(content, metrics, Some(input_content_rect(rect).w));
        let font_fingerprint = fonts.layout_fingerprint(None, paragraph.shaping);
        use_request(&fonts, request, font_fingerprint).map(Some)
    })
}

pub(crate) fn caret_and_scroll(
    layout: &TextLayout<'_>,
    editing: &TextEditing,
    content_rect: Rect,
) -> (Fixed, Fixed) {
    let caret = if editing.is_empty() {
        Fixed::ZERO
    } else {
        layout
            .carets()
            .iter()
            .find(|caret| caret.text_offset as usize == editing.cursor())
            .map_or(Fixed::ZERO, |caret| {
                crate::types::fixed::from_textflow(caret.position.x)
            })
    };
    let visible_width = (content_rect.w - Fixed::ONE).max(Fixed::ZERO);
    (caret, (caret - visible_width).max(Fixed::ZERO))
}

pub(super) fn hit_caret(world: &World, entity: Entity, point: Point) -> Option<usize> {
    let editing = world
        .get::<TextEditing>(entity)
        .filter(|editor| editor.usable(world))?;
    let (rect, affine, projective) =
        crate::ui::render_system::widget_render_geometry(world, entity)?;
    let point = if projective.is_identity() {
        affine.inverse()?.apply_point(point)
    } else {
        projective.inverse()?.apply_point(point)?
    };
    let handle = world.get::<TextEditingLayoutHandle>(entity)?.0;
    with_layout_request(world, entity, rect, |_, request, fingerprint| {
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
        if editing.is_empty() {
            return Ok(Some(0));
        }
        let content_rect = input_content_rect(rect);
        let (_, scroll) = caret_and_scroll(&layout, editing, content_rect);
        let x = point.x - content_rect.x + scroll;
        Ok(layout
            .carets()
            .iter()
            .min_by_key(|caret| (crate::types::fixed::from_textflow(caret.position.x) - x).abs())
            .map(|caret| caret.text_offset as usize))
    })
    .ok()
    .flatten()
    .flatten()
}
