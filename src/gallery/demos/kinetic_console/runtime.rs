use alloc::vec;

use super::composition::build_widgets;
use super::render::{orbit_view, wave_view};
use super::state::{ConsoleModel, ConsoleMotion, ConsoleNodes};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
use crate::types::DimPoint;

#[mirui_macros::system(order = ANIMATION)]
pub fn kinetic_animation_system(world: &mut World) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 450;

    let Some(state) = world.resource::<ConsoleModel>().map(ConsoleModel::snapshot) else {
        return;
    };
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(MAX_STEP_MS);
    let Some(motion) = world.resource_mut::<ConsoleMotion>() else {
        return;
    };
    let controls_changed = motion.state.mode != state.mode || motion.state.focused != state.focused;
    let intensity_changed = motion.state.intensity != state.intensity;
    motion.state = state;
    let previous_phase = motion.phase.phase();
    let speed = Fixed::from_int(state.mode.speed()) + state.intensity * Fixed::from_ratio(3, 5);
    let phase = motion.phase.advance(
        dt,
        RATE_RAMP_MS,
        speed,
        Fixed::ONE,
        state.paused,
        Fixed::from_int(360),
    );
    let moving = phase != previous_phase;
    if moving {
        motion.wave_elapsed_ms = motion.wave_elapsed_ms.saturating_add(dt);
    }
    let orbit_dirty = controls_changed || moving;
    let wave_dirty = controls_changed || intensity_changed || motion.wave_elapsed_ms >= 64;
    if wave_dirty {
        motion.wave_elapsed_ms = 0;
    }
    if let Some(nodes) = world
        .resource::<ConsoleNodes>()
        .map(|nodes| (nodes.orbit, nodes.wave))
    {
        if orbit_dirty {
            world.invalidate_visual(nodes.0);
        }
        if wave_dirty {
            world.invalidate_visual(nodes.1);
        }
    }
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
    app.world.insert_resource(ConsoleModel::default());
    app.world.insert_resource(ConsoleMotion::default());
    app.add_system(kinetic_animation_system::system());
    app.compose(parent, build_widgets);

    let orbit = app
        .world
        .find_by_id("kinetic_console_orbit_layer")
        .expect("Kinetic Console orbit layer");
    let wave = app
        .world
        .find_by_id("kinetic_console_wave_layer")
        .expect("Kinetic Console wave layer");
    app.world.insert_resource(ConsoleNodes { orbit, wave });
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
