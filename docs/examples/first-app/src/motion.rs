use super::model::Tasks;
use mirui::anim::{INTERACTIVE, Transition, TweenConfig, ease};
use mirui::prelude::*;
use mirui::render::command::DrawCommand;

#[component]
pub(crate) struct Progress(pub Fixed);

#[component]
pub(crate) struct ProgressTarget(pub Fixed);

pub(crate) struct MotionSettings(pub Transition);

// #region progress-motion
#[system(order = Animation)]
pub(crate) fn aim_progress(
    tasks: model!(Tasks),
    settings: res!(MotionSettings),
    meters: query!(Entity, mut ProgressTarget, Progress),
    commands: commands!(),
) {
    let goal = progress_ratio(tasks.completed(), tasks.total());
    for (entity, target, progress) in meters {
        if target.0 != goal {
            target.0 = goal;
            commands.insert(entity, AnimateProgress(settings.0.motion(progress.0, goal)));
        }
    }
}

#[animate(component = AnimateProgress)]
pub(crate) fn apply_progress(value: motion!(), progress: component!(mut Progress)) {
    progress.0 = value;
}
// #endregion progress-motion

#[view(component = Progress, invalidation = Visual)]
pub(crate) fn paint_progress(progress: component!(), theme: res!(Theme), paint: paint!()) {
    let mut area = paint.rect();
    area.w = area.w * progress.0.clamp(Fixed::ZERO, Fixed::ONE);
    if area.w > Fixed::ZERO {
        paint.draw(&DrawCommand::Fill {
            area,
            transform: paint.transform(),
            quad: None,
            color: ThemedColor::Token(ColorToken::Primary).resolve_in(&theme, paint.state()),
            radius: Fixed::from_int(3),
            opa: 255,
        });
    }
}

pub(crate) fn progress_transition(reduced: bool) -> Transition {
    TweenConfig::new(if reduced { 0 } else { 240 }, &ease::OUT_CUBIC).into()
}

pub(crate) fn indicator_transition(reduced: bool) -> Transition {
    if reduced {
        progress_transition(true)
    } else {
        INTERACTIVE.into()
    }
}

pub(crate) fn progress_ratio(completed: usize, total: usize) -> Fixed {
    if total == 0 {
        Fixed::ZERO
    } else {
        Fixed::from_ratio(completed as i32, total as i32)
    }
}
