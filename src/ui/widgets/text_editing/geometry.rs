use super::TextEditing;
use crate::ecs::{Entity, World};
use crate::text::TextGeometry;
use crate::types::{Fixed, Rect};
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
        fonts
            .with_typefaces(None, paragraph.shaping, |typefaces| {
                outputs.storage_mut().layouts.layout_cached(
                    u64::from(entity.id) | (u64::from(entity.generation) << 32),
                    font_fingerprint,
                    request,
                    typefaces,
                )
            })
            .map(Some)
            .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))
    })
}
