use super::geometry::CurvePaths;
use super::state::CurveModel;
use super::style::{BORDER, LANE_COLORS, PANEL};
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::path::PathStore;
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};

#[crate::component]
#[derive(Default)]
pub(super) struct CurveStage;

fn fill(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx<'_>,
    area: Rect,
    color: Color,
    radius: Fixed,
    opacity: u8,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area,
            transform: ctx.transform,
            quad: None,
            color,
            radius,
            opa: opacity,
        },
        ctx.clip,
    );
}

fn curve_stage_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<CurveStage>(entity) {
        return;
    }
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let panel = theme.resolve(PANEL);
    let border = theme.resolve(BORDER);
    let lane_colors = LANE_COLORS.map(|token| theme.resolve(token));
    ctx.bg_handled = true;
    fill(renderer, ctx, *rect, panel, Fixed::from_int(18), 255);
    for index in 0..12 {
        let x = rect.x + rect.w * Fixed::from_ratio(index, 11);
        fill(
            renderer,
            ctx,
            Rect::new(x, rect.y, Fixed::ONE, rect.h),
            border,
            Fixed::ZERO,
            if index % 3 == 0 { 48 } else { 22 },
        );
    }
    for index in 1..4 {
        let y = rect.y + rect.h * Fixed::from_ratio(index, 4);
        fill(
            renderer,
            ctx,
            Rect::new(rect.x, y, rect.w, Fixed::ONE),
            border,
            Fixed::ZERO,
            28,
        );
    }

    let phase = world
        .resource::<CurveModel>()
        .map_or(Fixed::ZERO, |model| model.phase.get_untracked());
    for index in 0..5 {
        let angle = phase + Fixed::from_int(index * 72);
        let x = rect.x + rect.w / 2 + Fixed::cos_deg(angle) * Fixed::from_int(300);
        let y = rect.y + rect.h / 2 + Fixed::sin_deg(angle * 2) * Fixed::from_int(126);
        let radius = Fixed::from_int(10 + index % 3 * 4);
        fill(
            renderer,
            ctx,
            Rect::new(x - radius, y - radius, radius * 2, radius * 2),
            lane_colors[index as usize % lane_colors.len()],
            radius,
            32,
        );
    }

    let (Some(paths), Some(store)) = (
        world.resource::<CurvePaths>().copied(),
        world.resource::<PathStore>(),
    ) else {
        return;
    };
    let transform = ctx.transform.compose(&Transform::translate(rect.x, rect.y));
    for (index, id) in paths.ids.into_iter().enumerate() {
        let Ok(path) = store.get(id) else {
            continue;
        };
        let paint = Paint::Color(lane_colors[index].into());
        ctx.draw(
            renderer,
            &DrawCommand::StrokePath {
                path,
                transform,
                paint: &paint,
                width: Fixed::from_int(5),
                opa: 16,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            },
            ctx.clip,
        );
        ctx.draw(
            renderer,
            &DrawCommand::StrokePath {
                path,
                transform,
                paint: &paint,
                width: Fixed::ONE,
                opa: 150,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            },
            ctx.clip,
        );
    }
}

pub(super) fn curve_stage_view() -> View {
    View::new("CurveStage", 60, curve_stage_render).with_filter::<CurveStage>()
}
