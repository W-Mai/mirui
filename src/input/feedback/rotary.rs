use crate::ecs::{Entity, World};
use crate::input::feedback::{InputFeedback, InputFeedbackInput, OverlayRotary};
use crate::render::canvas::Paint;
use crate::render::command::DrawCommand;
use crate::render::membrane::{MagneticMembrane, MagneticMembraneState};
use crate::types::{Color, Fixed, Rect, Viewport};
use crate::ui::dirty::Dirty;
use crate::ui::{Children, IgnoreHitTest, Parent, Style, Widget};

const PRIMARY: Color = Color::rgb(88, 166, 255);
/// Track width on the right edge. Wide enough for the membrane to swell
/// without clipping at default `max_amp = 28`.
const TRACK_WIDTH: i32 = 36;

/// Spring is "settled" when progress / velocity / target each fall below
/// this absolute value. 1/128 logical pixel is well under one display
/// pixel even at 4x scale, so the snap to zero is imperceptible.
fn settle_eps() -> Fixed {
    Fixed::ONE / Fixed::from_int(128)
}

fn rotary_active(rotary: &super::RotaryFeedback) -> bool {
    rotary.progress != Fixed::ZERO || rotary.opacity != Fixed::ZERO || rotary.pulse != Fixed::ZERO
}

fn rotary_track_rect(viewport: &Viewport) -> Rect {
    let (lw, lh) = viewport.logical_size();
    Rect::new(lw as i32 - TRACK_WIDTH, 0, TRACK_WIDTH, lh as i32)
}

pub(crate) fn rotary_membrane_state(
    rotary: &super::RotaryFeedback,
    membrane: &MagneticMembrane,
) -> MagneticMembraneState {
    let progress_amp = membrane.max_amp
        * (rotary.progress.abs() / membrane.span().max(Fixed::ONE)).min(Fixed::ONE);
    let pulse_amp = membrane.max_amp * rotary.pulse / Fixed::from_int(2);
    MagneticMembraneState {
        ball_offset: rotary.progress,
        amp: progress_amp.max(pulse_amp),
    }
}

pub(crate) fn spawn_overlay_rotary(world: &mut World, root: Entity) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, Widget);
    world.insert(entity, OverlayRotary);
    world.insert(entity, IgnoreHitTest);
    world.insert(entity, Style::absolute_at(Rect::ZERO));
    world.insert(entity, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(entity);
    } else {
        world.insert(root, Children(alloc::vec![entity]));
    }
    if let Some(mut feedback) = world.resource::<InputFeedback>().copied() {
        feedback.rotary.entity = Some(entity);
        world.insert_resource(feedback);
    }
    entity
}

#[crate::system(order = Normal)]
pub fn rotary_feedback_system(
    feedback: res!(optional InputFeedback),
    input: res!(optional InputFeedbackInput),
    delta: res!(optional crate::ecs::DeltaTimeMs),
    display: res!(optional crate::surface::DisplayInfo),
    overlays: query!(Entity, mut Style; with OverlayRotary;),
    commands: commands!(),
) {
    let Some(mut feedback) = feedback.copied() else {
        return;
    };
    if !feedback.rotary.enabled {
        return;
    }
    let before = feedback.rotary;
    let max_pull = MagneticMembrane::default().max_pull();

    let input = input.copied().unwrap_or_default();
    if input.event_seq != feedback.rotary.last_input_seq {
        let impulse = Fixed::from_int(input.rotary_delta as i32) - input.wheel_delta_y;
        if impulse != Fixed::ZERO {
            let next_target =
                (feedback.rotary.target + impulse * Fixed::from_int(10)).clamp(-max_pull, max_pull);
            feedback.rotary.velocity += (next_target - feedback.rotary.target) / Fixed::from_int(4);
            feedback.rotary.target = next_target;
            feedback.rotary.direction = if impulse > Fixed::ZERO { 1 } else { -1 };
            feedback.rotary.opacity = Fixed::ONE;
        }
        if input.click_pulse {
            feedback.rotary.pulse = Fixed::ONE;
            feedback.rotary.opacity = Fixed::ONE;
        }
        feedback.rotary.last_input_seq = input.event_seq;
        commands.insert_resource(InputFeedbackInput {
            event_seq: input.event_seq,
            ..InputFeedbackInput::default()
        });
    }

    let dt = delta.map(|delta| delta.0.max(1)).unwrap_or(16);
    let dt_fixed = Fixed::from_int(dt as i32) / Fixed::from_int(16);
    let spring =
        (feedback.rotary.target - feedback.rotary.progress) * dt_fixed / Fixed::from_int(3);
    feedback.rotary.velocity += spring;
    feedback.rotary.progress += feedback.rotary.velocity * dt_fixed / Fixed::from_int(4);
    feedback.rotary.progress = feedback.rotary.progress.clamp(-max_pull, max_pull);
    feedback.rotary.velocity = feedback.rotary.velocity * Fixed::from_int(21) / Fixed::from_int(25);
    feedback.rotary.target = feedback.rotary.target * Fixed::from_int(7) / Fixed::from_int(8);
    feedback.rotary.opacity =
        (feedback.rotary.opacity - dt_fixed / Fixed::from_int(25)).max(Fixed::ZERO);
    feedback.rotary.pulse =
        (feedback.rotary.pulse - dt_fixed / Fixed::from_int(16)).max(Fixed::ZERO);
    let settle = settle_eps();
    if feedback.rotary.progress.abs() < settle
        && feedback.rotary.velocity.abs() < settle
        && feedback.rotary.target.abs() < settle
    {
        feedback.rotary.progress = Fixed::ZERO;
        feedback.rotary.velocity = Fixed::ZERO;
        feedback.rotary.target = Fixed::ZERO;
    }
    if feedback.rotary == before {
        return;
    }
    commands.insert_resource(feedback);

    let Some(entity) = feedback.rotary.entity else {
        return;
    };

    let viewport = display.map(|info| info.viewport());
    let target_rect = match viewport {
        Some(vp) if rotary_active(&feedback.rotary) => rotary_track_rect(&vp),
        _ => Rect::ZERO,
    };
    for (overlay, style) in overlays {
        if overlay != entity {
            continue;
        }
        style.layout.left = crate::types::Dimension::Px(target_rect.x);
        style.layout.top = crate::types::Dimension::Px(target_rect.y);
        style.layout.width = crate::types::Dimension::Px(target_rect.w);
        style.layout.height = crate::types::Dimension::Px(target_rect.h);
        commands.insert(overlay, Dirty);
        break;
    }
}

#[crate::view(component = OverlayRotary, name = "input_feedback_rotary", priority = 91)]
fn rotary_render(feedback: res!(optional InputFeedback), paint_ctx: paint!()) {
    let Some(feedback) = feedback else {
        return;
    };
    if !feedback.rotary.enabled || !rotary_active(&feedback.rotary) {
        return;
    }
    paint_ctx.set_background_handled(true);
    let rect = paint_ctx.rect();

    let mut membrane = MagneticMembrane::default();
    let max_span = rect.h / Fixed::from_int(2) - Fixed::from_int(18);
    membrane.visible_span = ((max_span.max(Fixed::ONE) / membrane.sigma) * Fixed::from_int(7)
        / Fixed::from_int(10))
    .max(Fixed::ONE);

    let mid_y = rect.y + rect.h / Fixed::from_int(2);
    let edge_x = rect.x + rect.w;
    // Floor at 25% so a freshly-settled membrane is still faintly visible.
    let opacity = feedback.rotary.opacity.max(Fixed::ONE / Fixed::from_int(4));
    let opa = opacity.map01(255).to_int() as u8;
    let path = membrane.path(
        edge_x,
        mid_y,
        rotary_membrane_state(&feedback.rotary, &membrane),
    );
    let fill = Paint::Color(PRIMARY.into());
    paint_ctx.draw(&DrawCommand::FillPath {
        path: &path,
        transform: paint_ctx.transform(),
        paint: &fill,
        opa,
        fill_rule: crate::render::raster::FillRule::EvenOdd,
    });
}

pub use rotary_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::DeltaTimeMs;
    use crate::render::renderer::{RenderError, RenderFeature, Renderer};
    use crate::types::Transform;
    use crate::ui::WidgetRoot;
    use crate::ui::view::PaintCtx;

    fn make_world() -> World {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        app.world
    }

    fn world_with_rotary_root() -> World {
        let mut world = make_world();
        world.insert_resource(InputFeedback::enabled());
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, Style::default());
        world.insert_resource(WidgetRoot(root));
        spawn_overlay_rotary(&mut world, root);
        world.insert_resource(DeltaTimeMs(16));
        world
    }

    fn run_rotary_feedback(world: &mut World) {
        let mut scheduler = crate::ecs::SystemScheduler::new();
        scheduler.add(rotary_feedback_system::system());
        scheduler.run_all(world).unwrap();
    }

    #[test]
    fn rotary_draw_failure_reaches_paint_context() {
        struct RejectRenderer;
        impl Renderer for RejectRenderer {
            fn submit(
                &mut self,
                _: &crate::render::renderer::DrawRequest<'_, '_>,
            ) -> Result<(), RenderError> {
                Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
            }

            fn flush(&mut self) {}
        }

        let mut world = world_with_rotary_root();
        let mut feedback = world.resource::<InputFeedback>().copied().unwrap();
        feedback.rotary.opacity = Fixed::ONE;
        let entity = feedback.rotary.entity.unwrap();
        world.insert_resource(feedback);
        let style = Style::default();
        let rect = Rect::new(0, 0, 64, 64);
        let mut renderer = RejectRenderer;
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            crate::ui::theme::WidgetState::Enabled,
        );
        view().render().unwrap()(&world, entity, &mut paint);
        assert_eq!(
            paint.error(),
            Some(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
    }

    #[test]
    fn consumes_rotary_and_wheel_impulses() {
        let mut world = world_with_rotary_root();
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 2,
            wheel_delta_y: Fixed::from_int(1),
            click_pulse: true,
            event_seq: 1,
        });

        run_rotary_feedback(&mut world);

        let feedback = world.resource::<InputFeedback>().unwrap();
        assert!(feedback.rotary.progress != Fixed::ZERO || feedback.rotary.velocity != Fixed::ZERO);
        assert_eq!(feedback.rotary.direction, 1);
        assert!(feedback.rotary.opacity > Fixed::ZERO);
        assert!(feedback.rotary.pulse > Fixed::ZERO);
    }

    #[test]
    fn consumes_each_input_batch_once() {
        let mut world = world_with_rotary_root();
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 1,
            wheel_delta_y: Fixed::ZERO,
            click_pulse: false,
            event_seq: 1,
        });

        run_rotary_feedback(&mut world);
        let first_target = world.resource::<InputFeedback>().unwrap().rotary.target;
        run_rotary_feedback(&mut world);
        let second_target = world.resource::<InputFeedback>().unwrap().rotary.target;

        // target decays *7/8 per frame; double-consume would re-add the +10 impulse.
        let upper = first_target * Fixed::from_int(95) / Fixed::from_int(100);
        assert!(second_target < upper);
        let input = world.resource::<InputFeedbackInput>().unwrap();
        assert_eq!(input.rotary_delta, 0);
        assert_eq!(input.wheel_delta_y, Fixed::ZERO);
        assert!(!input.click_pulse);
    }

    #[test]
    fn bounded_stretch_under_fast_input() {
        let mut world = world_with_rotary_root();
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 0,
            wheel_delta_y: Fixed::from_int(100),
            click_pulse: false,
            event_seq: 1,
        });

        run_rotary_feedback(&mut world);
        let feedback = world.resource::<InputFeedback>().unwrap();
        let max = MagneticMembrane::default().max_pull();
        assert!(feedback.rotary.target.abs() <= max);
        assert!(feedback.rotary.progress.abs() <= max);
        assert!(feedback.rotary.progress < Fixed::ZERO);

        for _ in 0..19 {
            run_rotary_feedback(&mut world);
        }
        let feedback = world.resource::<InputFeedback>().unwrap();
        assert!(feedback.rotary.progress.abs() <= max);
    }

    #[test]
    fn membrane_state_preserves_progress_sign() {
        let membrane = MagneticMembrane::default();
        let up = super::super::RotaryFeedback {
            progress: Fixed::from_int(12),
            direction: 1,
            ..super::super::RotaryFeedback::default()
        };
        let down = super::super::RotaryFeedback {
            progress: Fixed::from_int(-12),
            direction: -1,
            ..super::super::RotaryFeedback::default()
        };

        assert!(rotary_membrane_state(&up, &membrane).ball_offset > Fixed::ZERO);
        assert!(rotary_membrane_state(&down, &membrane).ball_offset < Fixed::ZERO);
    }

    #[test]
    fn click_pulse_creates_membrane_amplitude() {
        let membrane = MagneticMembrane::default();
        let click = super::super::RotaryFeedback {
            pulse: Fixed::ONE,
            ..super::super::RotaryFeedback::default()
        };
        assert!(rotary_membrane_state(&click, &membrane).amp > Fixed::ZERO);
    }

    #[test]
    fn marks_overlay_dirty_when_state_changes() {
        let mut world = world_with_rotary_root();
        let entity = world
            .resource::<InputFeedback>()
            .unwrap()
            .rotary
            .entity
            .expect("spawned");
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 1,
            wheel_delta_y: Fixed::ZERO,
            click_pulse: false,
            event_seq: 1,
        });

        run_rotary_feedback(&mut world);
        assert!(world.get::<Dirty>(entity).is_some());
    }
}
