use super::geometry::{CurvePaths, path_window_end, update_lane_for_stage};
use super::stage::CurveStage;
use super::state::{CurveModel, CurveStageSize};
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::{LayoutAxis, LayoutDependency};

#[crate::component]
#[derive(Default)]
pub(super) struct CurveMotion(super::super::motion::BrakedPhase);

#[compose(bind(model))]
pub(super) fn bind_stage_layout(
    stage: Entity,
    primary: Entity,
    multiscript: Entity,
    caption: Entity,
    paths: CurvePaths,
    model: CurveModel,
) {
    let dependencies = [
        LayoutDependency::entity(stage, LayoutAxis::Width),
        LayoutDependency::entity(stage, LayoutAxis::Height),
    ];
    cx.bind_layout(stage, &dependencies, move |world, _, values| {
        let size = CurveStageSize::from_dimensions(values.get(0), values.get(1));
        model.set_stage_size(size);

        let end = path_window_end(size.width);
        for (entity, path) in [
            (primary, paths.ids[0]),
            (multiscript, paths.ids[1]),
            (caption, paths.ids[2]),
        ] {
            world
                .widget_mut(entity)
                .expect("responsive curve path target")
                .text_path(crate::text::TextPath::new(path).with_range(Fixed::ZERO..end));
        }
    });
}

#[mirui_macros::system(order = ANIMATION, expect = CurveStage)]
pub(super) fn curve_text_animation_system(world: &mut World) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 520;

    let dt = world
        .resource::<DeltaTimeMs>()
        .expect("Curve Text animation requires DeltaTimeMs")
        .0
        .min(MAX_STEP_MS);
    world.for_each_stable::<CurveStage>(|world, entity| {
        let model = world
            .get::<CurveStage>(entity)
            .expect("CurveStage disappeared during animation")
            .model
            .clone();
        let (paused, reversed, speed) = crate::core::model::with_model_read_only(|| {
            crate::core::model::ModelHandle::read(&model, |model| {
                (model.paused(), model.reversed(), model.speed())
            })
        });
        let phase = {
            let motion = world
                .get_mut::<CurveMotion>(entity)
                .expect("CurveStage requires CurveMotion");
            let direction = if reversed {
                Fixed::from_int(-1)
            } else {
                Fixed::ONE
            };
            motion.0.advance(
                dt,
                RATE_RAMP_MS,
                speed,
                direction,
                paused,
                Fixed::from_int(360),
            )
        };
        model.set_phase(phase);
    });
}

#[compose(bind(model))]
pub(super) fn bind_curve_paths(stage: Entity, paths: CurvePaths, model: CurveModel) {
    for (lane, path) in paths.ids.into_iter().enumerate() {
        let model = model.clone();
        cx.bind_path_visual(stage, path, move |geometry| {
            let current_phase = model.phase();
            let stage_size = model.stage_size();
            let envelope = Fixed::from_ratio(17, 20)
                + Fixed::sin_deg(current_phase / 3 + Fixed::from_int(lane as i32 * 37))
                    * Fixed::from_ratio(3, 20);
            update_lane_for_stage(
                geometry,
                lane,
                current_phase,
                model.amplitude() * envelope,
                stage_size.width,
                stage_size.height,
            );
        })
        .expect("mutable curve path");
    }
}

#[cfg(test)]
impl CurveMotion {
    pub(super) fn rate(&self) -> Fixed {
        self.0.rate()
    }
}
