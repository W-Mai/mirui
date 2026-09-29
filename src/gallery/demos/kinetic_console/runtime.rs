use alloc::vec;

use super::composition::build_widgets;
use super::render::{orbit_view, wave_view};
use super::state::{ConsoleModel, ConsoleMotion};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::run_order::ANIMATION;
use crate::ecs::{DeltaTimeMs, System};
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
use crate::types::DimPoint;

type ConsoleHandle = <ConsoleModel as crate::core::model::Model>::Handle;

pub(super) fn advance_motion(
    model: &ConsoleHandle,
    motion: &Signal<ConsoleMotion>,
    delta: DeltaTimeMs,
) {
    let mut next = motion.get_untracked();
    if next.advance(model.mode(), model.intensity(), model.paused(), delta.0) {
        motion.set(next);
    }
}

pub(super) fn kinetic_animation_system(
    model: ConsoleHandle,
    motion: Signal<ConsoleMotion>,
) -> System {
    let bound_model = model.clone();
    System::bound(
        "kinetic_animation_system",
        ANIMATION,
        &model,
        move |world| {
            let delta = world
                .resource::<DeltaTimeMs>()
                .copied()
                .expect("Kinetic Console requires DeltaTimeMs");
            advance_motion(&bound_model, &motion, delta);
        },
    )
}

pub fn build_sim_timeline(world: &World) -> Option<SimTimeline> {
    let orbit = world.find_by_id("kinetic_console_orbit")?;
    let flow = world.find_by_id("kinetic_console_flow")?;
    let pulse = world.find_by_id("kinetic_console_pulse")?;
    let orbit_layer = world.find_by_id("kinetic_console_orbit_layer")?;
    let status = world.find_by_id("kinetic_console_status")?;
    let slider = world.find_by_id("kinetic_console_intensity")?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(900),
            SimAction::tap(DimPoint::CENTER).on(flow),
            SimAction::wait(700),
            SimAction::drag(
                DimPoint::percent(20, 50),
                DimPoint::percent(88, 50),
                900,
                crate::anim::ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(orbit_layer),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(pulse),
            SimAction::wait(900),
            SimAction::tap(DimPoint::CENTER).on(status),
            SimAction::wait(2_800),
            SimAction::tap(DimPoint::CENTER).on(status),
            SimAction::wait(500),
            SimAction::tap(DimPoint::CENTER).on(orbit_layer),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(orbit),
            SimAction::wait(700),
            SimAction::drag(
                DimPoint::percent(88, 50),
                DimPoint::percent(32, 50),
                800,
                crate::anim::ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(900),
        ])
        .looping(true),
    )
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity, autoplay: bool)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    app.with_widget(orbit_view()).with_widget(wave_view());
    let model = app.add_model(ConsoleModel::default());
    let motion = Signal::new(ConsoleMotion::default());
    app.add_system(kinetic_animation_system(model.clone(), motion.clone()));
    app.compose(parent, |cx| build_widgets(cx, model, motion));

    if autoplay && let Some(timeline) = build_sim_timeline(&app.world) {
        app.world.insert_resource(timeline);
        app.add_system(sim_timeline_system::system());
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    install(app, parent, true);
}
