#![allow(clippy::needless_update)]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::ui::dirty::Dirty;

#[derive(Default)]
pub struct Butterfly {
    pub start_ms: u32,
}

fn wing_path(cx: Fixed, cy: Fixed, span: Fixed, tilt: Fixed, side: i32, inner: bool) -> Path {
    let s = Fixed::from_int(side);
    let shrink = if inner {
        Fixed::from_f32(0.6)
    } else {
        Fixed::ONE
    };

    let anchor_top = Point {
        x: cx + tilt * Fixed::from_int(4),
        y: cy - Fixed::from_int(8) * shrink,
    };
    let anchor_bot = Point {
        x: cx - tilt * Fixed::from_int(4),
        y: cy + Fixed::from_int(10) * shrink,
    };

    let forewing_tip = Point {
        x: cx + Fixed::from_int(44) * s * span * shrink,
        y: cy - Fixed::from_int(24) * shrink,
    };
    let hindwing_tip = Point {
        x: cx + Fixed::from_int(34) * s * span * shrink,
        y: cy + Fixed::from_int(22) * shrink,
    };

    let fw_out_c1 = Point {
        x: cx + Fixed::from_int(18) * s * span * shrink,
        y: anchor_top.y - Fixed::from_int(22) * shrink,
    };
    let fw_out_c2 = Point {
        x: cx + Fixed::from_int(52) * s * span * shrink,
        y: forewing_tip.y - Fixed::from_int(6) * shrink,
    };
    let fw_in_c1 = Point {
        x: cx + Fixed::from_int(46) * s * span * shrink,
        y: cy - Fixed::from_int(6) * shrink,
    };
    let fw_in_c2 = Point {
        x: cx + Fixed::from_int(14) * s * span * shrink,
        y: cy - Fixed::from_int(1) * shrink,
    };
    let notch = Point {
        x: cx + Fixed::from_int(10) * s * span * shrink,
        y: cy + Fixed::from_int(4) * shrink,
    };
    let hw_out_c1 = Point {
        x: cx + Fixed::from_int(36) * s * span * shrink,
        y: cy + Fixed::from_int(6) * shrink,
    };
    let hw_out_c2 = Point {
        x: cx + Fixed::from_int(40) * s * span * shrink,
        y: hindwing_tip.y - Fixed::from_int(2) * shrink,
    };
    let hw_in_c1 = Point {
        x: cx + Fixed::from_int(26) * s * span * shrink,
        y: cy + Fixed::from_int(18) * shrink,
    };
    let hw_in_c2 = Point {
        x: cx + Fixed::from_int(8) * s * span * shrink,
        y: cy + Fixed::from_int(14) * shrink,
    };

    let mut path = Path::new();
    path.move_to(anchor_top);
    path.cubic_to(fw_out_c1, fw_out_c2, forewing_tip);
    path.cubic_to(fw_in_c1, fw_in_c2, notch);
    path.cubic_to(hw_out_c1, hw_out_c2, hindwing_tip);
    path.cubic_to(hw_in_c1, hw_in_c2, anchor_bot);
    path.close();
    path
}

fn fill_wing(
    renderer: &mut dyn Renderer,
    clip: &Rect,
    transform: crate::types::Transform,
    cx: Fixed,
    cy: Fixed,
    span: Fixed,
    tilt: Fixed,
    side: i32,
    inner: bool,
) {
    let path = wing_path(cx, cy, span, tilt, side, inner);
    let color = if inner {
        if side < 0 {
            Color::rgb(130, 210, 240)
        } else {
            Color::rgb(150, 220, 245)
        }
    } else if side < 0 {
        Color::rgb(40, 70, 160)
    } else {
        Color::rgb(50, 80, 170)
    };
    let opa = if inner { 210 } else { 240 };
    renderer.draw(
        &DrawCommand::FillPath {
            path: &path,
            transform,
            color,
            opa,
        },
        clip,
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
        renderer,
        ctx.clip,
        ctx.transform,
        cx,
        cy,
        span_left,
        tilt,
        -1,
        false,
    );
    fill_wing(
        renderer,
        ctx.clip,
        ctx.transform,
        cx,
        cy,
        span_right,
        tilt,
        1,
        false,
    );
    fill_wing(
        renderer,
        ctx.clip,
        ctx.transform,
        cx,
        cy,
        span_left,
        tilt,
        -1,
        true,
    );
    fill_wing(
        renderer,
        ctx.clip,
        ctx.transform,
        cx,
        cy,
        span_right,
        tilt,
        1,
        true,
    );

    let body_head = Point {
        x: cx + tilt * Fixed::from_int(6),
        y: cy - Fixed::from_int(14),
    };
    let body_tail = Point {
        x: cx - tilt * Fixed::from_int(6),
        y: cy + Fixed::from_int(16),
    };
    renderer.draw(
        &DrawCommand::Line {
            p1: body_head,
            p2: body_tail,
            transform: ctx.transform,
            color: Color::rgb(30, 20, 40),
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );
    renderer.draw(
        &DrawCommand::Line {
            p1: body_head,
            p2: Point {
                x: body_head.x - Fixed::from_int(5),
                y: body_head.y - Fixed::from_int(10),
            },
            transform: ctx.transform,
            color: Color::rgb(50, 40, 60),
            width: Fixed::ONE,
            opa: 220,
        },
        ctx.clip,
    );
    renderer.draw(
        &DrawCommand::Line {
            p1: body_head,
            p2: Point {
                x: body_head.x + Fixed::from_int(5),
                y: body_head.y - Fixed::from_int(10),
            },
            transform: ctx.transform,
            color: Color::rgb(50, 40, 60),
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
    let mut buf = alloc::vec::Vec::new();
    world.query::<Butterfly>().collect_into(&mut buf);
    for e in buf {
        world.insert(e, Dirty);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Butterfly (
            start_ms: now_ms,
            grow: 1.0
        )
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
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::view::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(butterfly_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
