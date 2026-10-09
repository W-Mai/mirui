use super::geometry::{CurvePaths, update_lane_for_stage};
use super::stage::CurveStage;
use super::state::CurveModel;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;

#[crate::component]
#[derive(Default)]
pub(super) struct CurveMotion(super::super::motion::BrakedPhase);

#[mirui_macros::system(order = Animation, expect = CurveStage)]
pub(super) fn curve_text_animation_system(
    delta: res!(DeltaTimeMs),
    stages: query!(Entity; with CurveStage;),
    animated: query!(CurveStage, mut CurveMotion),
) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 520;

    let dt = delta.0.min(MAX_STEP_MS);
    let expected = stages.into_iter().count();
    let mut updated = 0usize;
    for (stage, motion) in animated {
        updated += 1;
        let model = &stage.model;
        let (paused, reversed, speed) = crate::core::model::with_model_read_only(|| {
            crate::core::model::ModelHandle::read(model, |model| {
                (model.paused(), model.reversed(), model.speed())
            })
        });
        let direction = if reversed {
            Fixed::from_int(-1)
        } else {
            Fixed::ONE
        };
        let phase = motion.0.advance(
            dt,
            RATE_RAMP_MS,
            speed,
            direction,
            paused,
            Fixed::from_int(360),
        );
        model.set_phase(phase);
    }
    assert_eq!(updated, expected, "CurveStage requires CurveMotion");
}

#[compose]
pub(super) fn bind_curve_paths(stage: Entity, paths: CurvePaths, model: model!(CurveModel)) {
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
