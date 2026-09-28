use super::scene::{CLIP_CIRCLE, LOGICAL_SIZE};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::renderer::DrawRequest;
use crate::render::scene::Paint;
use crate::types::Transform;
use crate::ui::Theme;

#[crate::component]
#[derive(Default)]
pub struct ClipPath;

fn fill_rect(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    area: Rect,
    color: Color,
    transform: Transform,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area,
            transform,
            quad: None,
            color,
            radius: Fixed::from_int(10),
            opa: 255,
        },
        ctx.clip,
    );
}

pub(super) fn clip_path_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let canvas =
        crate::gallery::fit_logical_canvas(*rect, ctx.transform, LOGICAL_SIZE, LOGICAL_SIZE);
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let surface = theme.resolve(ColorToken::SurfaceVariant);
    let foreground = theme.resolve(ColorToken::OnSurface);
    let grays = [
        surface,
        surface.blend_with(foreground, Fixed::from_ratio(1, 12)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 6)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 4)),
    ];
    let colors = [
        theme.resolve(ColorToken::Primary),
        theme.resolve(ColorToken::Secondary),
        theme.resolve(ColorToken::Tertiary),
        theme.resolve(ColorToken::Success),
    ];

    for (i, color) in grays.into_iter().enumerate() {
        fill_rect(
            renderer,
            ctx,
            Rect::new(30, 42 + i as i32 * 56, 260, 42),
            color,
            canvas,
        );
    }
    if ctx.error.is_some() {
        return;
    }

    let circle_transform = canvas.compose(&Transform::translate(
        Fixed::from_int(56),
        Fixed::from_int(56),
    ));
    ctx.draw(
        renderer,
        &DrawCommand::PushClip {
            path: &CLIP_CIRCLE,
            transform: circle_transform,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
    if ctx.error.is_some() {
        return;
    }
    for (i, color) in colors.into_iter().enumerate() {
        fill_rect(
            renderer,
            ctx,
            Rect::new(30, 42 + i as i32 * 56, 260, 42),
            color,
            canvas,
        );
    }
    ctx.record(renderer.submit(&DrawRequest::new(&DrawCommand::PopClip, *ctx.clip)));

    let outline = Paint::Color(theme.resolve(ColorToken::OnSurface).into());
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &CLIP_CIRCLE,
            transform: circle_transform,
            paint: &outline,
            width: Fixed::from_int(2),
            opa: 220,
            line_cap: crate::render::scene::LineCap::Round,
            line_join: crate::render::scene::LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        },
        ctx.clip,
    );
}

pub fn clip_path_view() -> View {
    View::new("ClipPath", 60, clip_path_render).with_filter::<ClipPath>()
}
