#![allow(clippy::needless_update)]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::canvas::Paint;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[derive(Default)]
pub struct Butterfly {
    pub start_ms: u32,
}

static WING: Path = path!(
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

#[mirui_macros::system(order = ANIMATION)]
pub fn butterfly_anim_system(world: &mut World) {
    world.for_each_stable::<Butterfly>(|world, e| {
        world.invalidate(e);
    });
}

#[compose]
pub fn build_widgets() {
    let now_ms = cx
        .world_mut()
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(12),
            bg_color: ColorToken::Surface
        ) {
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 440,
                max_height: 296,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: 38,
                    padding: Padding {
                        left: Dimension::px(14),
                        right: Dimension::px(14),
                        ..Default::default()
                    },
                    align: AlignItems::Center
                ) {
                    Text (
                        "FLIGHT STUDY",
                        grow: 1.0,
                        font_size: 14,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "VECTOR · LIVE",
                        width: 112,
                        height: 22,
                        font_size: 9,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Secondary,
                        border_color: ColorToken::Secondary,
                        border_width: 1,
                        border_radius: 11,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Butterfly (
                    start_ms: now_ms,
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100)
                )
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(butterfly_view());
    app.add_system(butterfly_anim_system::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::view::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(butterfly_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn wing_geometry_stays_in_static_storage() {
        assert!(WING.is_borrowed());
    }
}
