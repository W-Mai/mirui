use super::binding::slider_to_progress_system;
use super::composition::build_widgets;
use super::state::{ThemeCycle, cycle, dark_with_accent};
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::sim::{SimAction, SimTimeline};
#[cfg(feature = "std")]
use crate::prelude::plugin::{FpsSummaryPlugin, InputFeedbackPlugin};
use crate::prelude::*;
use crate::types::DimPoint;
use crate::ui::IdMap;
use alloc::vec;

/// Install the workbench, theme cycle, and optional input simulation.
///
/// Register the default widgets and systems and provide a `MonoClock` before running the app.
pub fn install<B, F>(app: &mut App<B, F>, parent: Entity, autoplay: bool)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(crate::input::event::sim::sim_timeline_system::system());
    app.add_system(slider_to_progress_system::system());
    app.add_resource(dark_with_accent());
    let theme_cycle = app.add_model(ThemeCycle::default());
    app.add_timer(cycle::timer(theme_cycle));
    app.compose(parent, build_widgets());

    if autoplay && let Some(timeline) = app.compose(parent, build_sim_timeline()) {
        app.add_resource(timeline);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(InputFeedbackPlugin::default());
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(FpsSummaryPlugin::default());
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.with_offscreen_pool_budget(512 * 1024);
    let autoplay = std::env::var("MIRUI_SIM_OFF").ok().as_deref() != Some("1");
    install(app, parent, autoplay);
}

/// Prepare the demo's looping input sequence from its registered widget IDs.
///
/// Returns `None` when the expected widgets aren't installed. Install a returned timeline with `App::add_resource`; the sim system is a no-op without one.
#[compose]
pub fn build_sim_timeline() -> Option<SimTimeline> {
    let (tab_list, tab_form, tab_theme, switch_e, slider_e, list_e) = res!(IdMap).with(|ids| {
        Some((
            ids.get("widgets_tab_list")?,
            ids.get("widgets_tab_form")?,
            ids.get("widgets_tab_theme")?,
            ids.get("widgets_switch")?,
            ids.get("widgets_slider")?,
            ids.get("widgets_list")?,
        ))
    })?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(tab_form),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(10, 50),
                DimPoint::percent(90, 50),
                600,
                ease::ease_in_out_cubic,
            )
            .on(slider_e),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(1500),
            SimAction::tap(DimPoint::CENTER).on(tab_theme),
            SimAction::wait(6500),
            SimAction::tap(DimPoint::CENTER).on(tab_list),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 80),
                DimPoint::percent(50, 20),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 20),
                DimPoint::percent(50, 80),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
        ])
        .looping(true),
    )
}
