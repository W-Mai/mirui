use super::geometry::{CurvePaths, path_window_end, update_lane_for_stage};
use super::state::{CurveModel, CurveNodes, CurveStageSize};
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::{LayoutAxis, LayoutDependency};

pub(super) type CurveMotion = super::super::motion::BrakedPhase;

pub(super) fn bind_stage_layout(
    cx: &mut crate::ui::UiScope<'_>,
    nodes: CurveNodes,
    paths: CurvePaths,
    model: CurveModel,
) {
    let dependencies = [
        LayoutDependency::entity(nodes.stage, LayoutAxis::Width),
        LayoutDependency::entity(nodes.stage, LayoutAxis::Height),
    ];
    cx.bind_layout(nodes.stage, &dependencies, move |world, _, values| {
        let size = CurveStageSize::from_dimensions(values.get(0), values.get(1));
        model.stage_size.set(size);

        let end = path_window_end(size.width);
        for (entity, path) in [
            (nodes.primary, paths.ids[0]),
            (nodes.multiscript, paths.ids[1]),
            (nodes.caption, paths.ids[2]),
        ] {
            world
                .widget_mut(entity)
                .expect("responsive curve path target")
                .text_path(crate::text::TextPath::new(path).with_range(Fixed::ZERO..end));
        }
        world.invalidate_visual(nodes.stage);
    });
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn curve_text_animation_system(world: &mut World) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 520;

    let Some(model) = world.resource::<CurveModel>().cloned() else {
        return;
    };
    let paused = model.paused.get_untracked();
    let reversed = model.reversed.get_untracked();
    let speed = model.speed.get_untracked();
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(MAX_STEP_MS);
    let Some(motion) = world.resource_mut::<CurveMotion>() else {
        return;
    };
    let direction = if reversed {
        Fixed::from_int(-1)
    } else {
        Fixed::ONE
    };
    let phase = motion.advance(
        dt,
        RATE_RAMP_MS,
        speed,
        direction,
        paused,
        Fixed::from_int(360),
    );
    if phase == model.phase.get_untracked() {
        return;
    }
    model.phase.set(phase);
}

pub(super) fn bind_curve_paths(
    cx: &mut crate::ui::UiScope<'_>,
    stage: Entity,
    paths: CurvePaths,
    model: &CurveModel,
) {
    for (lane, path) in paths.ids.into_iter().enumerate() {
        let phase = model.phase.clone();
        let amplitude = model.amplitude.clone();
        let stage_size = model.stage_size.clone();
        cx.bind_path_visual(stage, path, move |geometry| {
            let current_phase = phase.get();
            let stage_size = stage_size.get();
            let envelope = Fixed::from_ratio(17, 20)
                + Fixed::sin_deg(current_phase / 3 + Fixed::from_int(lane as i32 * 37))
                    * Fixed::from_ratio(3, 20);
            update_lane_for_stage(
                geometry,
                lane,
                current_phase,
                amplitude.get() * envelope,
                stage_size.width,
                stage_size.height,
            );
        })
        .expect("mutable curve path");
    }
}
