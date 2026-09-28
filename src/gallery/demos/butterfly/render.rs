use super::state::Butterfly;
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::canvas::Paint;
use crate::types::Transform;
use crate::ui::Theme;

pub(super) static WING: Path = path!(
    M 0 22
    C 18 0 52 0 44 6
    C 46 24 14 29 10 34
    C 36 36 40 50 34 52
    C 26 48 8 44 0 40
    Z
);

fn fill_wing(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    cx: Fixed,
    cy: Fixed,
    span: Fixed,
    tilt: Fixed,
    side: i32,
    inner: bool,
    outer_color: Color,
    inner_color: Color,
) {
    let color = if inner { inner_color } else { outer_color };
    let opa = if inner { 210 } else { 240 };
    let paint = Paint::Color(color.into());
    let shrink = if inner {
        Fixed::from_f32(0.6)
    } else {
        Fixed::ONE
    };
    let shear = Fixed::ZERO - tilt / Fixed::from_int(2);
    let local_y = Fixed::from_int(30);
    let wing_transform = Transform {
        m00: Fixed::from_int(side) * span * shrink,
        m01: shear,
        tx: cx - shear * local_y,
        m10: Fixed::ZERO,
        m11: shrink,
        ty: cy - shrink * local_y,
    };
    ctx.draw(
        renderer,
        &DrawCommand::FillPath {
            path: &WING,
            transform: ctx.transform.compose(&wing_transform),
            paint: &paint,
            opa,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        },
        ctx.clip,
    );
}

//~focus-start
fn butterfly_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(state) = world.get::<Butterfly>(entity) else {
        return;
    };
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let outer_wing = theme.resolve(ColorToken::Primary);
    let inner_wing = theme.resolve(ColorToken::Secondary);
    let body_color = theme.resolve(ColorToken::OnSurface);
    let detail_color = theme.resolve(ColorToken::OnSurfaceVariant);
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;

    let amp_x = rect.w / Fixed::from_int(4);
    let amp_y = rect.h / Fixed::from_int(5);
    let tx_deg = Fixed::from_int((elapsed_ms * 360 / 3100) % 360);
    let ty_deg = Fixed::from_int((elapsed_ms * 360 / 1900) % 360);
    let cx = rect.x + rect.w / Fixed::from_int(2) + Fixed::sin_deg(tx_deg) * amp_x;
    let cy = rect.y + rect.h / Fixed::from_int(2) + Fixed::sin_deg(ty_deg) * amp_y;
    let tilt = Fixed::cos_deg(tx_deg) * Fixed::from_f32(0.35);
    let yaw_deg = Fixed::from_int((elapsed_ms * 360 / 2400) % 360);
    let yaw = Fixed::sin_deg(yaw_deg) * Fixed::from_f32(0.55);

    let flap_deg = Fixed::from_int((elapsed_ms * 360 / 280) % 360);
    let raw = Fixed::sin_deg(flap_deg).abs();
    let span_base = Fixed::from_f32(0.25) + raw * Fixed::from_f32(0.75);

    let min_span = Fixed::from_f32(0.15);
    let span_left = (span_base * (Fixed::ONE + yaw)).max(min_span);
    let span_right = (span_base * (Fixed::ONE - yaw)).max(min_span);

    fill_wing(
        renderer, ctx, cx, cy, span_left, tilt, -1, false, outer_wing, inner_wing,
    );
    fill_wing(
        renderer, ctx, cx, cy, span_right, tilt, 1, false, outer_wing, inner_wing,
    );
    fill_wing(
        renderer, ctx, cx, cy, span_left, tilt, -1, true, outer_wing, inner_wing,
    );
    fill_wing(
        renderer, ctx, cx, cy, span_right, tilt, 1, true, outer_wing, inner_wing,
    );

    let body_head = Point {
        x: cx + tilt * Fixed::from_int(6),
        y: cy - Fixed::from_int(14),
    };
    let body_tail = Point {
        x: cx - tilt * Fixed::from_int(6),
        y: cy + Fixed::from_int(16),
    };
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1: body_head,
            p2: body_tail,
            transform: ctx.transform,
            color: body_color,
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1: body_head,
            p2: Point {
                x: body_head.x - Fixed::from_int(5),
                y: body_head.y - Fixed::from_int(10),
            },
            transform: ctx.transform,
            color: detail_color,
            width: Fixed::ONE,
            opa: 220,
        },
        ctx.clip,
    );
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1: body_head,
            p2: Point {
                x: body_head.x + Fixed::from_int(5),
                y: body_head.y - Fixed::from_int(10),
            },
            transform: ctx.transform,
            color: detail_color,
            width: Fixed::ONE,
            opa: 220,
        },
        ctx.clip,
    );
}
//~focus-end

pub fn butterfly_view() -> View {
    View::new("Butterfly", 60, butterfly_render).with_filter::<Butterfly>()
}
