use crate::draw::command::DrawCommand;
use crate::draw::membrane::{MagneticMembrane, MagneticMembraneState};
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::feedback::{InputFeedback, InputFeedbackInput, OverlayRotary};
use crate::types::{Color, Dimension, Fixed, Rect, Viewport};
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};
use crate::widget::{Children, IgnoreHitTest, Parent, Style, Widget};

const PRIMARY: Color = Color::rgb(88, 166, 255);
/// Track width on the right edge. Wide enough for the membrane to swell
/// without clipping at default `max_amp = 28`.
const TRACK_WIDTH: i32 = 36;

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

fn write_layout(world: &mut World, entity: Entity, rect: Rect) {
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = Dimension::Px(rect.x);
        style.layout.top = Dimension::Px(rect.y);
        style.layout.width = Dimension::Px(rect.w);
        style.layout.height = Dimension::Px(rect.h);
    }
}

pub(crate) fn spawn_overlay_rotary(world: &mut World, root: Entity) -> Entity {
    let entity = world.spawn();
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

#[crate::system(order = NORMAL)]
pub fn rotary_feedback_system(world: &mut World) {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return;
    };
    if !feedback.rotary.enabled {
        return;
    }
    let before = feedback.rotary;

    let input = world
        .resource::<InputFeedbackInput>()
        .copied()
        .unwrap_or_default();
    if input.event_seq != feedback.rotary.last_input_seq {
        let impulse = Fixed::from_int(input.rotary_delta as i32) - input.wheel_delta_y;
        if impulse != Fixed::ZERO {
            let max_stretch = MagneticMembrane::default().max_pull();
            let next_target = (feedback.rotary.target + impulse * Fixed::from_int(10))
                .clamp(Fixed::ZERO - max_stretch, max_stretch);
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
        world.insert_resource(InputFeedbackInput {
            event_seq: input.event_seq,
            ..InputFeedbackInput::default()
        });
    }

    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map(|d| d.0.max(1))
        .unwrap_or(16);
    let dt_fixed = Fixed::from_int(dt as i32) / Fixed::from_int(16);
    let spring =
        (feedback.rotary.target - feedback.rotary.progress) * dt_fixed / Fixed::from_int(3);
    feedback.rotary.velocity += spring;
    feedback.rotary.progress += feedback.rotary.velocity * dt_fixed / Fixed::from_int(4);
    let max_pull = MagneticMembrane::default().max_pull();
    feedback.rotary.progress = feedback
        .rotary
        .progress
        .clamp(Fixed::ZERO - max_pull, max_pull);
    feedback.rotary.velocity = feedback.rotary.velocity * Fixed::from_int(21) / Fixed::from_int(25);
    feedback.rotary.target = feedback.rotary.target * Fixed::from_int(7) / Fixed::from_int(8);
    feedback.rotary.opacity =
        (feedback.rotary.opacity - dt_fixed / Fixed::from_int(25)).max(Fixed::ZERO);
    feedback.rotary.pulse =
        (feedback.rotary.pulse - dt_fixed / Fixed::from_int(16)).max(Fixed::ZERO);
    let settle = Fixed::ONE / Fixed::from_int(128);
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
    world.insert_resource(feedback);

    let Some(entity) = feedback.rotary.entity else {
        return;
    };

    let viewport = world
        .resource::<crate::surface::DisplayInfo>()
        .map(|info| info.viewport());
    let target_rect = match viewport {
        Some(vp) if rotary_active(&feedback.rotary) => rotary_track_rect(&vp),
        _ => Rect::ZERO,
    };
    write_layout(world, entity, target_rect);
    world.insert(entity, Dirty);
}

fn render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if world.get::<OverlayRotary>(entity).is_none() {
        return;
    }
    let Some(feedback) = world.resource::<InputFeedback>() else {
        return;
    };
    if !feedback.rotary.enabled || !rotary_active(&feedback.rotary) {
        return;
    }
    ctx.bg_handled = true;

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
    renderer.draw(
        &DrawCommand::FillPath {
            path: &path,
            transform: ctx.transform,
            color: PRIMARY,
            opa,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("input_feedback_rotary", 91, render)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::DeltaTimeMs;
    use crate::widget::WidgetRoot;

    fn make_world() -> World {
        crate::app::App::headless(64, 64)
            .with_default_widgets()
            .world
    }

    fn world_with_rotary_root() -> World {
        let mut world = make_world();
        world.insert_resource(InputFeedback::enabled());
        let root = world.spawn();
        world.insert(root, Widget);
        world.insert(root, Style::default());
        world.insert_resource(WidgetRoot(root));
        spawn_overlay_rotary(&mut world, root);
        world.insert_resource(DeltaTimeMs(16));
        world
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

        rotary_feedback_system(&mut world);

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

        rotary_feedback_system(&mut world);
        let first_target = world.resource::<InputFeedback>().unwrap().rotary.target;
        rotary_feedback_system(&mut world);
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

        rotary_feedback_system(&mut world);
        let feedback = world.resource::<InputFeedback>().unwrap();
        let max = MagneticMembrane::default().max_pull();
        assert!(feedback.rotary.target.abs() <= max);
        assert!(feedback.rotary.progress.abs() <= max);
        assert!(feedback.rotary.progress < Fixed::ZERO);

        for _ in 0..19 {
            rotary_feedback_system(&mut world);
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

        rotary_feedback_system(&mut world);
        assert!(world.get::<Dirty>(entity).is_some());
    }
}
